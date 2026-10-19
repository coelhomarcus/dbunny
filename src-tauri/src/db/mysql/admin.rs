use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Error as MysqlError, Row};

use super::catalog::{number, quote, string, text};
use super::MysqlDriver;
use crate::db::model::{DatabaseInfo, RoleInfo, RoleList, ServerActivity, ServerSession};
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;
use crate::db::{unsupported, Driver, Engine, ServerAdmin};

/// Databases every server has; they are never listed.
const SYSTEM_DATABASES: &str = "'mysql', 'sys', 'information_schema', 'performance_schema'";

/// The accounts MySQL makes for its own use.
const BUILTIN_ACCOUNTS: [&str; 3] = ["mysql.sys", "mysql.session", "mysql.infoschema"];

/// How long dropping a database waits for the locks other sessions hold. The server's own
/// wait is a year.
const DROP_LOCK_WAIT: Duration = Duration::from_secs(10);

/// The server answers a `KILL` for a session that is already gone with this error.
const UNKNOWN_THREAD: u16 = 1094;

/// Waiting for a lock longer than `lock_wait_timeout` allows.
const LOCK_WAIT_TIMEOUT: u16 = 1205;

/// The errors for a table or a privilege the account lacks (1044 and 1142: no SELECT, 1227: no PROCESS).
const DENIED: [u16; 4] = [1044, 1142, 1143, 1227];

/// One row of the process list, whichever way it was read.
struct RawSession {
    id: i64,
    user: Option<String>,
    host: Option<String>,
    database: Option<String>,
    command: String,
    seconds: Option<f64>,
    state: Option<String>,
    query: Option<String>,
    /// Seconds since the open transaction started; only the full picture knows
    transaction_seconds: Option<f64>,
    application: Option<String>,
}

/// The sessions with their transactions and applications: the performance schema and
/// `INNODB_TRX`. `database` limits them to those using it.
async fn rich_sessions(conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
    let filter = if database.is_some() { "WHERE p.DB = ?" } else { "" };
    let sql = format!(
        "SELECT p.ID AS id, p.USER AS user_name, p.HOST AS host, p.DB AS db_name, p.COMMAND AS command, \
                p.TIME AS seconds, p.STATE AS state, p.INFO AS info, \
                TIMESTAMPDIFF(SECOND, trx.trx_started, NOW()) AS trx_seconds, \
                COALESCE(program.ATTR_VALUE, client.ATTR_VALUE) AS application \
         FROM performance_schema.processlist p \
         LEFT JOIN information_schema.INNODB_TRX trx ON trx.trx_mysql_thread_id = p.ID \
         LEFT JOIN performance_schema.session_connect_attrs program ON program.PROCESSLIST_ID = p.ID AND program.ATTR_NAME = 'program_name' \
         LEFT JOIN performance_schema.session_connect_attrs client ON client.PROCESSLIST_ID = p.ID AND client.ATTR_NAME = '_client_name' \
         {filter} ORDER BY p.ID"
    );
    let rows: Vec<Row> = match database {
        Some(database) => conn.exec(sql, (database,)).await?,
        None => conn.query(sql).await?,
    };
    Ok(rows
        .iter()
        .map(|row| RawSession {
            id: number(row, "id").unwrap_or(0),
            user: text(row, "user_name"),
            host: text(row, "host"),
            database: text(row, "db_name"),
            command: string(row, "command"),
            seconds: text(row, "seconds").and_then(|s| s.parse().ok()),
            state: text(row, "state"),
            query: text(row, "info"),
            transaction_seconds: number(row, "trx_seconds").map(|s| s as f64),
            application: text(row, "application"),
        })
        .collect())
}

/// `SHOW FULL PROCESSLIST`, which any account may run (it lists its own sessions).
async fn plain_sessions(conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
    let rows: Vec<Row> = conn.query("SHOW FULL PROCESSLIST").await?;
    Ok(rows
        .iter()
        .filter(|row| database.is_none() || text(row, "db").as_deref() == database)
        .map(|row| RawSession {
            id: number(row, "Id").unwrap_or(0),
            user: text(row, "User"),
            host: text(row, "Host"),
            database: text(row, "db"),
            command: string(row, "Command"),
            seconds: text(row, "Time").and_then(|s| s.parse().ok()),
            state: text(row, "State"),
            query: text(row, "Info"),
            transaction_seconds: None,
            application: None,
        })
        .collect())
}

/// What the global grants of an account allow of the things the Roles page shows.
#[derive(Debug, Default, PartialEq)]
struct GlobalPrivileges {
    superuser: bool,
    create_db: bool,
    create_role: bool,
    replication: bool,
}

/// Reads `SHOW GRANTS` lines for the privileges granted on `*.*`.
fn global_privileges(grants: &[String]) -> GlobalPrivileges {
    let mut found = GlobalPrivileges::default();
    for grant in grants {
        let Some((list, _)) = grant.strip_prefix("GRANT ").and_then(|rest| rest.split_once(" ON *.* TO ")) else { continue };
        for privilege in list.split(", ").map(str::trim) {
            match privilege {
                "ALL PRIVILEGES" | "ALL" => found = GlobalPrivileges { superuser: true, create_db: true, create_role: true, replication: true },
                "SUPER" => found.superuser = true,
                "CREATE" => found.create_db = true,
                "CREATE USER" | "CREATE ROLE" => found.create_role = true,
                "REPLICATION SLAVE" => found.replication = true,
                _ => {}
            }
        }
    }
    found
}

/// The accounts of the server. An account that is locked, or that another is granted as a role,
/// can't log in: it is a role. The server's own accounts are the built-in ones.
async fn accounts(conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>> {
    let rows: Vec<Row> = conn
        .query(
            "SELECT u.User AS name, u.Host AS host, u.account_locked AS locked, u.password_expired AS expired, \
                    u.Super_priv AS super_priv, u.Create_priv AS create_priv, u.Create_user_priv AS create_user_priv, \
                    u.Repl_slave_priv AS repl_priv, u.max_user_connections AS max_connections, \
                    CONCAT(u.User, '@', u.Host) = CURRENT_USER() AS is_current, \
                    (SELECT GROUP_CONCAT(IF(e.FROM_HOST = '%', e.FROM_USER, CONCAT(e.FROM_USER, '@', e.FROM_HOST)) ORDER BY e.FROM_USER SEPARATOR ',') \
                       FROM mysql.role_edges e WHERE e.TO_USER = u.User AND e.TO_HOST = u.Host) AS member_of, \
                    EXISTS (SELECT 1 FROM mysql.role_edges e WHERE e.FROM_USER = u.User AND e.FROM_HOST = u.Host) AS is_role \
             FROM mysql.user u",
        )
        .await?;
    let mut accounts: Vec<RoleInfo> = rows
        .iter()
        .map(|row| {
            let name = string(row, "name");
            let can_login = string(row, "locked") != "Y" && number(row, "is_role") != Some(1);
            let limit = number(row, "max_connections").unwrap_or(0);
            RoleInfo {
                is_builtin: BUILTIN_ACCOUNTS.contains(&name.as_str()),
                name,
                host: text(row, "host"),
                can_login,
                superuser: string(row, "super_priv") == "Y",
                create_db: string(row, "create_priv") == "Y",
                create_role: string(row, "create_user_priv") == "Y",
                replication: string(row, "repl_priv") == "Y",
                bypass_rls: None,
                connection_limit: if limit > 0 { limit as i32 } else { -1 },
                valid_until: (can_login && string(row, "expired") == "Y").then(|| "expired".to_string()),
                member_of: text(row, "member_of").map(|names| names.split(',').map(String::from).collect()).unwrap_or_default(),
                is_current: number(row, "is_current") == Some(1),
            }
        })
        .collect();
    accounts.sort_by(|a, b| (a.is_builtin, !a.can_login, &a.name, &a.host).cmp(&(b.is_builtin, !b.can_login, &b.name, &b.host)));
    Ok(accounts)
}

/// Only the account of this session, for one that can't read the `mysql` schema: its global
/// privileges come from its own grants.
async fn own_account(conn: &mut mysql_async::Conn) -> mysql_async::Result<RoleInfo> {
    let current: String = conn.query_first("SELECT CURRENT_USER()").await?.unwrap_or_default();
    let (name, host) = current.rsplit_once('@').map_or((current.clone(), None), |(name, host)| (name.to_string(), Some(host.to_string())));
    let grants: Vec<String> = conn.query("SHOW GRANTS").await?;
    let privileges = global_privileges(&grants);
    Ok(RoleInfo {
        name,
        host,
        can_login: true,
        superuser: privileges.superuser,
        create_db: privileges.create_db,
        create_role: privileges.create_role,
        replication: privileges.replication,
        bypass_rls: None,
        connection_limit: -1,
        valid_until: None,
        member_of: vec![],
        is_current: true,
        is_builtin: false,
    })
}

fn database_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The database name can't be empty".into());
    }
    Ok(name)
}

/// DROP DATABASE waiting at most `wait` for the locks other sessions hold. With `force` the
/// other sessions that use the database are ended first. The one the session itself is open
/// on stays: its pool would lose it from under the app.
pub(super) async fn drop_database_waiting(driver: &MysqlDriver, name: &str, force: bool, wait: Duration) -> Result<(), String> {
    let name = database_name(name)?;
    if name == driver.database() {
        return Err(format!("This session is open on {name}: open another database first."));
    }
    let mut conn = driver.conn().await?;
    let own = [i64::from(driver.editor().connection_id())];
    if force {
        let sessions: Vec<i64> = conn
            .exec("SELECT ID FROM performance_schema.processlist WHERE DB = ? AND ID <> CONNECTION_ID()", (name,))
            .await
            .map_err(|e| error_message(&e, driver.endpoint()))?;
        for id in sessions.into_iter().filter(|id| !own.contains(id)) {
            // One that finished meanwhile is fine
            let _ = conn.query_drop(format!("KILL CONNECTION {id}")).await;
        }
    }
    conn.query_drop(format!("SET SESSION lock_wait_timeout = {}", wait.as_secs().max(1))).await.map_err(|e| error_message(&e, driver.endpoint()))?;
    let dropped = conn.query_drop(format!("DROP DATABASE {}", quote(name))).await;
    // The connection goes back to the pool: it keeps no short wait
    let _ = conn.query_drop("SET SESSION lock_wait_timeout = DEFAULT").await;
    dropped.map_err(|e| match &e {
        MysqlError::Server(server) if server.code == LOCK_WAIT_TIMEOUT => {
            format!("Another session is using {name}. Try again, or end the other sessions.")
        }
        other => error_message(other, driver.endpoint()),
    })
}

#[async_trait]
impl ServerAdmin for MysqlDriver {
    /// The databases the user can see, without the server's own.
    async fn databases(&self) -> Result<Vec<DatabaseInfo>, String> {
        let mut conn = self.conn().await?;
        let rows: Vec<Row> = conn
            .query(format!(
                "SELECT s.SCHEMA_NAME AS name, SUM(t.DATA_LENGTH + t.INDEX_LENGTH) AS size_bytes \
                 FROM information_schema.SCHEMATA s \
                 LEFT JOIN information_schema.TABLES t ON t.TABLE_SCHEMA = s.SCHEMA_NAME \
                 WHERE s.SCHEMA_NAME NOT IN ({SYSTEM_DATABASES}) GROUP BY s.SCHEMA_NAME ORDER BY s.SCHEMA_NAME"
            ))
            .await
            .map_err(|e| error_message(&e, self.endpoint()))?;
        Ok(rows
            .iter()
            .map(|row| {
                let name = string(row, "name");
                DatabaseInfo { is_current: name == self.database(), name, size_bytes: Some(number(row, "size_bytes").unwrap_or(0)), can_connect: true }
            })
            .collect())
    }

    /// Another database of the same server, with the same login, SSL, read-only and SSH
    /// settings. The SSH host key was trusted when this session opened, so it isn't asked
    /// for again.
    async fn open_database(&self, name: &str) -> Result<Arc<dyn Driver>, String> {
        if name.trim().is_empty() {
            return Err("Choose a database".into());
        }
        let endpoint = MysqlEndpoint { database: Some(name.trim().to_string()), ..self.endpoint().clone() };
        let driver: Arc<dyn Driver> = MysqlDriver::open(endpoint, None).await?;
        Ok(driver)
    }

    async fn create_database(&self, name: &str) -> Result<(), String> {
        let name = database_name(name)?;
        let mut conn = self.conn().await?;
        conn.query_drop(format!("CREATE DATABASE {}", quote(name))).await.map_err(|e| error_message(&e, self.endpoint()))
    }

    async fn drop_database(&self, name: &str, force: bool) -> Result<(), String> {
        drop_database_waiting(self, name, force, DROP_LOCK_WAIT).await
    }

    async fn create_schema(&self, _name: &str) -> Result<(), String> {
        Err(unsupported(Engine::Mysql, "Schemas"))
    }

    async fn rename_schema(&self, _name: &str, _new_name: &str) -> Result<(), String> {
        Err(unsupported(Engine::Mysql, "Schemas"))
    }

    async fn drop_schema(&self, _name: &str, _cascade: bool) -> Result<(), String> {
        Err(unsupported(Engine::Mysql, "Schemas"))
    }

    async fn roles(&self) -> Result<RoleList, String> {
        let mut conn = self.conn().await?;
        match accounts(&mut conn).await {
            Ok(roles) => Ok(RoleList { roles, limited_view: false }),
            Err(MysqlError::Server(server)) if DENIED.contains(&server.code) => {
                let own = own_account(&mut conn).await.map_err(|e| error_message(&e, self.endpoint()))?;
                Ok(RoleList { roles: vec![own], limited_view: true })
            }
            Err(e) => Err(error_message(&e, self.endpoint())),
        }
    }

    /// What the server is doing: its sessions (the process list), which of them hold a
    /// transaction open and which wait on another's row locks.
    async fn activity(&self, all_databases: bool) -> Result<ServerActivity, String> {
        let mut conn = self.conn().await?;
        let fail = |e: MysqlError| error_message(&e, self.endpoint());
        let own = conn.id();
        let editor = self.editor().connection_id();

        // The full picture needs the performance schema and, for transactions, PROCESS; an
        // account without them still gets its own sessions from the plain process list
        let scope = if all_databases { None } else { Some(self.database()) };
        let sessions = match rich_sessions(&mut conn, scope).await {
            Ok(rows) => rows,
            Err(MysqlError::Server(server)) if DENIED.contains(&server.code) => plain_sessions(&mut conn, scope).await.map_err(fail)?,
            Err(e) => return Err(fail(e)),
        };

        // Who waits on whom; without the permission to see it nobody is shown as blocked
        let waits: Vec<(Option<i64>, Option<i64>)> = conn
            .query(
                "SELECT DISTINCT waiting.PROCESSLIST_ID, blocking.PROCESSLIST_ID FROM performance_schema.data_lock_waits w \
                 JOIN performance_schema.threads waiting ON waiting.THREAD_ID = w.REQUESTING_THREAD_ID \
                 JOIN performance_schema.threads blocking ON blocking.THREAD_ID = w.BLOCKING_THREAD_ID",
            )
            .await
            .unwrap_or_default();
        let mut blocked_by: HashMap<i64, Vec<i64>> = HashMap::new();
        for (waiting, blocking) in waits {
            if let (Some(waiting), Some(blocking)) = (waiting, blocking) {
                blocked_by.entry(waiting).or_default().push(blocking);
            }
        }

        // Without PROCESS the server lists only the account's own sessions
        let process: Option<i64> = conn
            .query_first(
                "SELECT COUNT(*) FROM information_schema.USER_PRIVILEGES WHERE PRIVILEGE_TYPE = 'PROCESS' \
                 AND GRANTEE = CONCAT(CHAR(39), SUBSTRING_INDEX(CURRENT_USER(), '@', 1), CHAR(39), '@', CHAR(39), SUBSTRING_INDEX(CURRENT_USER(), '@', -1), CHAR(39))",
            )
            .await
            .unwrap_or(None);

        let sessions = sessions
            .into_iter()
            .map(|raw| {
                let blockers = blocked_by.remove(&raw.id).unwrap_or_default();
                let sleeping = raw.command == "Sleep";
                let state = raw.state.clone().unwrap_or_default();
                let waiting_for = if !blockers.is_empty() {
                    Some("Lock: row lock".to_string())
                } else if state.starts_with("Waiting for") {
                    Some(state)
                } else {
                    None
                };
                ServerSession {
                    pid: raw.id,
                    user: raw.user,
                    database: raw.database,
                    application: raw.application,
                    client: raw.host.map(|host| host.rsplit_once(':').map_or(host.clone(), |(name, _)| name.to_string())),
                    state: Some(
                        match (sleeping, raw.transaction_seconds.is_some()) {
                            (true, true) => "idle in transaction",
                            (true, false) => "idle",
                            (false, _) => "active",
                        }
                        .to_string(),
                    ),
                    waiting_for,
                    query_seconds: raw.seconds,
                    transaction_seconds: raw.transaction_seconds,
                    query: raw.query,
                    blocked_by: blockers,
                    is_self: raw.id == i64::from(editor) || raw.id == i64::from(own),
                }
            })
            .collect();
        Ok(ServerActivity { sessions, limited_view: process == Some(0) })
    }

    /// Cancels the session's running query (`terminate` false) or ends the session. DBunny's
    /// own connections are left alone. A session that is already gone answers false.
    async fn signal_backend(&self, pid: i64, terminate: bool) -> Result<bool, String> {
        let id = u64::try_from(pid).ok().filter(|id| *id > 0).ok_or_else(|| format!("Invalid process id {pid}"))?;
        let mut conn = self.conn().await?;
        let own = [i64::from(self.editor().connection_id()), i64::from(conn.id())];
        if own.contains(&pid) {
            return Err("That's DBunny's own connection".into());
        }
        let kill = if terminate { "KILL CONNECTION" } else { "KILL QUERY" };
        match conn.query_drop(format!("{kill} {id}")).await {
            Ok(()) => Ok(true),
            Err(MysqlError::Server(server)) if server.code == UNKNOWN_THREAD => Ok(false),
            Err(e) => Err(error_message(&e, self.endpoint())),
        }
    }
}

#[cfg(test)]
mod server_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use mysql_async::prelude::Queryable;
    use mysql_async::Conn;

    use super::super::tests_support::{exec, raw_conn, test_endpoint, TestDb};
    use super::super::MysqlDriver;
    use crate::db::mysql_common::connect::MysqlEndpoint;
    use crate::db::Driver;

    async fn open() -> (TestDb, Arc<MysqlDriver>) {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        (db, driver)
    }

    /// Drops a database or an account when the test ends, also when it fails.
    struct Cleanup(String);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let sql = self.0.clone();
            let _ = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                runtime.block_on(exec(&test_endpoint(), &sql));
            })
            .join();
        }
    }

    async fn id_of(conn: &mut Conn) -> i64 {
        conn.query_first::<i64, _>("SELECT CONNECTION_ID()").await.unwrap().unwrap()
    }

    /// A connection of another program that runs `sql` in the background. The connection comes
    /// back with the result, so it stays open while the test looks at it.
    async fn busy(endpoint: &MysqlEndpoint, sql: &'static str) -> (i64, tokio::task::JoinHandle<(Result<(), mysql_async::Error>, Conn)>) {
        let mut conn = raw_conn(endpoint).await;
        let id = id_of(&mut conn).await;
        (id, tokio::spawn(async move { (conn.query_drop(sql).await, conn) }))
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn databases_are_listed_created_opened_and_dropped() {
        let (db, d) = open().await;
        let admin = d.admin().expect("mysql administers its server");
        let other = format!("{}_new", db.name);
        let _cleanup = Cleanup(format!("DROP DATABASE IF EXISTS `{other}`"));

        admin.create_database(&other).await.unwrap();
        let listed = admin.databases().await.unwrap();
        let names: Vec<&str> = listed.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&db.name.as_str()) && names.contains(&other.as_str()));
        for system in ["mysql", "sys", "information_schema", "performance_schema"] {
            assert!(!names.contains(&system), "{system} is not listed");
        }
        let (current, created) = (listed.iter().find(|d| d.name == db.name).unwrap(), listed.iter().find(|d| d.name == other).unwrap());
        assert!(current.is_current && !created.is_current && created.can_connect);
        assert_eq!(created.size_bytes, Some(0));

        let opened = admin.open_database(&other).await.unwrap();
        assert_eq!(opened.info().database, other);
        assert_eq!(opened.engine(), crate::db::Engine::Mysql);
        opened.close().await;

        assert!(admin.drop_database(&other, false).await.is_ok());
        assert!(!admin.databases().await.unwrap().iter().any(|d| d.name == other));
        // The one the session is on stays
        let err = admin.drop_database(&db.name, false).await.err().unwrap();
        assert_eq!(err, format!("This session is open on {}: open another database first.", db.name));
        assert_eq!(admin.create_database("  ").await.err().unwrap(), "The database name can't be empty");
        assert_eq!(admin.open_database(" ").await.err().unwrap(), "Choose a database");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn force_drops_a_database_other_sessions_use() {
        let (db, d) = open().await;
        let admin = d.admin().unwrap();
        let other = format!("{}_busy", db.name);
        let _cleanup = Cleanup(format!("DROP DATABASE IF EXISTS `{other}`"));
        admin.create_database(&other).await.unwrap();

        // Another program holds a lock on a table of it
        let mut holder = raw_conn(&MysqlEndpoint { database: Some(other.clone()), ..test_endpoint() }).await;
        holder.query_drop("CREATE TABLE t (a INT)").await.unwrap();
        holder.query_drop("LOCK TABLES t READ").await.unwrap();

        // Without force the server makes the drop wait for the lock, and the wait has an end
        let started = Instant::now();
        let waited = super::drop_database_waiting(&d, &other, false, Duration::from_secs(1)).await;
        assert!(started.elapsed() < Duration::from_secs(8), "the wait is bounded");
        assert_eq!(waited.err().unwrap(), format!("Another session is using {other}. Try again, or end the other sessions."));

        let started = Instant::now();
        admin.drop_database(&other, true).await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(10), "force doesn't wait for the lock");
        assert!(!admin.databases().await.unwrap().iter().any(|d| d.name == other));
        // The holder's connection was ended
        assert!(holder.query_drop("SELECT 1").await.is_err());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn activity_lists_sessions_and_kills_them() {
        let (db, d) = open().await;
        let admin = d.admin().unwrap();
        let (sleeper_id, sleeper) = busy(&db.endpoint, "SELECT SLEEP(30)").await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        let activity = admin.activity(false).await.unwrap();
        let session = activity.sessions.iter().find(|s| s.pid == sleeper_id).expect("the sleeping session is listed");
        assert_eq!(session.state.as_deref(), Some("active"));
        assert_eq!(session.query.as_deref(), Some("SELECT SLEEP(30)"));
        assert!(session.query_seconds.unwrap() >= 0.0);
        assert_eq!(session.database.as_deref(), Some(db.name.as_str()));
        assert_eq!(session.user.as_deref(), Some("root"));
        assert!(!session.is_self && session.blocked_by.is_empty());
        assert!(activity.sessions.iter().any(|s| s.is_self), "the connection that asked is DBunny's own");
        assert!(!activity.limited_view);

        // Another program on no database is listed only when asked for every database
        let mut elsewhere = raw_conn(&MysqlEndpoint { database: None, ..test_endpoint() }).await;
        let elsewhere_id = id_of(&mut elsewhere).await;
        assert!(!admin.activity(false).await.unwrap().sessions.iter().any(|s| s.pid == elsewhere_id));
        assert!(admin.activity(true).await.unwrap().sessions.iter().any(|s| s.pid == elsewhere_id));
        assert_eq!(admin.activity(true).await.unwrap().sessions.iter().find(|s| s.pid == elsewhere_id).unwrap().state.as_deref(), Some("idle"));

        // Cancelling the query ends the query, not the session
        assert!(admin.signal_backend(sleeper_id, false).await.unwrap());
        let (_, _kept_open) = tokio::time::timeout(Duration::from_secs(5), sleeper).await.expect("the query ended").unwrap();
        assert!(admin.activity(true).await.unwrap().sessions.iter().any(|s| s.pid == sleeper_id), "the session is still there");

        // Ending the session closes it
        let (second_id, second) = busy(&db.endpoint, "SELECT SLEEP(30)").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(admin.signal_backend(second_id, true).await.unwrap());
        let (ended, _) = tokio::time::timeout(Duration::from_secs(5), second).await.expect("the query ended").unwrap();
        assert!(ended.is_err(), "the connection went with it");
        // A session that is gone is not an error
        assert!(!admin.signal_backend(second_id, true).await.unwrap());
        assert_eq!(admin.signal_backend(0, true).await.err().unwrap(), "Invalid process id 0");
        // DBunny's own connections are left alone
        let own = d.editor().connection_id() as i64;
        assert_eq!(admin.signal_backend(own, true).await.err().unwrap(), "That's DBunny's own connection");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn activity_shows_who_blocks_whom() {
        let (db, d) = open().await;
        let admin = d.admin().unwrap();
        exec(&db.endpoint, "CREATE TABLE t (id INT PRIMARY KEY, v INT); INSERT INTO t VALUES (1, 0)").await;
        let mut holder = raw_conn(&db.endpoint).await;
        let holder_id = id_of(&mut holder).await;
        holder.query_drop("START TRANSACTION").await.unwrap();
        holder.query_drop("UPDATE t SET v = 1 WHERE id = 1").await.unwrap();
        let (waiter_id, waiter) = busy(&db.endpoint, "UPDATE t SET v = 2 WHERE id = 1").await;
        tokio::time::sleep(Duration::from_millis(700)).await;

        let activity = admin.activity(false).await.unwrap();
        let find = |id: i64| activity.sessions.iter().find(|s| s.pid == id).unwrap_or_else(|| panic!("no session {id}"));
        let holding = find(holder_id);
        assert_eq!(holding.state.as_deref(), Some("idle in transaction"));
        assert!(holding.transaction_seconds.is_some());
        let waiting = find(waiter_id);
        assert_eq!(waiting.blocked_by, vec![holder_id]);
        assert_eq!(waiting.state.as_deref(), Some("active"));
        assert!(waiting.waiting_for.as_deref().unwrap_or("").starts_with("Lock"), "{:?}", waiting.waiting_for);
        assert!(waiting.query.as_deref().unwrap_or("").contains("SET v = 2"));

        holder.query_drop("ROLLBACK").await.unwrap();
        let (waited, _) = tokio::time::timeout(Duration::from_secs(5), waiter).await.expect("the lock was released").unwrap();
        waited.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn without_process_privilege_the_view_is_limited() {
        let db = TestDb::new().await;
        let user = format!("dbunny_t_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let _cleanup = Cleanup(format!("DROP USER IF EXISTS '{user}'@'%'"));
        exec(&test_endpoint(), &format!("CREATE USER '{user}'@'%'; GRANT SELECT ON `{}`.* TO '{user}'@'%'", db.name)).await;
        // A session of someone else that the limited user must not see
        let (stranger_id, _stranger) = busy(&db.endpoint, "SELECT SLEEP(5)").await;
        let d = MysqlDriver::open(MysqlEndpoint { user: user.clone(), password: String::new(), ..db.endpoint.clone() }, None).await.unwrap();
        let admin = d.admin().unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let activity = admin.activity(true).await.unwrap();
        assert!(activity.limited_view);
        assert!(activity.sessions.iter().all(|s| s.user.as_deref() == Some(user.as_str())), "only its own sessions");
        assert!(!activity.sessions.iter().any(|s| s.pid == stranger_id));
        assert!(activity.sessions.iter().any(|s| s.is_self));
        d.close().await;
    }

    fn unique(prefix: &str) -> String {
        format!("{prefix}_{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn roles_map_mysql_accounts() {
        let (db, d) = open().await;
        let (user, role) = (unique("dbunny_t_u"), unique("dbunny_t_r"));
        let _users = (Cleanup(format!("DROP USER IF EXISTS '{user}'@'%'")), Cleanup(format!("DROP ROLE IF EXISTS '{role}'@'%'")));
        exec(
            &test_endpoint(),
            &format!(
                "CREATE USER '{user}'@'%' WITH MAX_USER_CONNECTIONS 3; CREATE ROLE '{role}'; \
                 GRANT '{role}' TO '{user}'@'%'; GRANT CREATE ON *.* TO '{user}'@'%'"
            ),
        )
        .await;
        let list = d.admin().unwrap().roles().await.unwrap();
        assert!(!list.limited_view);
        let find = |name: &str| list.roles.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no account {name}"));

        let account = find(&user);
        assert_eq!(account.host.as_deref(), Some("%"));
        assert!(account.can_login && account.create_db && !account.superuser && !account.create_role && !account.replication);
        assert_eq!(account.connection_limit, 3);
        assert_eq!(account.member_of, vec![role.clone()]);
        assert_eq!((account.bypass_rls, account.valid_until.as_deref(), account.is_builtin, account.is_current), (None, None, false, false));

        let group = find(&role);
        assert!(!group.can_login && group.member_of.is_empty());

        let root = find("root");
        assert!(root.superuser && root.create_role && root.replication && root.is_current && root.can_login);
        assert_eq!(root.connection_limit, -1);
        for builtin in ["mysql.sys", "mysql.session", "mysql.infoschema"] {
            assert!(find(builtin).is_builtin, "{builtin}");
        }
        // The server's own accounts come last
        let first_builtin = list.roles.iter().position(|r| r.is_builtin).unwrap();
        assert!(list.roles[first_builtin..].iter().all(|r| r.is_builtin));
        let _ = db;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn an_expired_password_is_shown() {
        let (_db, d) = open().await;
        let user = unique("dbunny_t_x");
        let _cleanup = Cleanup(format!("DROP USER IF EXISTS '{user}'@'%'"));
        exec(&test_endpoint(), &format!("CREATE USER '{user}'@'%' PASSWORD EXPIRE")).await;
        let list = d.admin().unwrap().roles().await.unwrap();
        let account = list.roles.iter().find(|r| r.name == user).unwrap();
        assert_eq!(account.valid_until.as_deref(), Some("expired"));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn without_access_to_mysql_user_only_the_own_account_shows() {
        let db = TestDb::new().await;
        let user = unique("dbunny_t_l");
        let _cleanup = Cleanup(format!("DROP USER IF EXISTS '{user}'@'%'"));
        exec(&test_endpoint(), &format!("CREATE USER '{user}'@'%'; GRANT SELECT ON `{}`.* TO '{user}'@'%'", db.name)).await;
        let d = MysqlDriver::open(MysqlEndpoint { user: user.clone(), password: String::new(), ..db.endpoint.clone() }, None).await.unwrap();
        let list = d.admin().unwrap().roles().await.unwrap();
        assert!(list.limited_view);
        assert_eq!(list.roles.len(), 1);
        let own = &list.roles[0];
        assert_eq!((own.name.as_str(), own.is_current, own.can_login, own.superuser), (user.as_str(), true, true, false));
        assert!(own.host.is_some());
        d.close().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn schemas_are_unsupported() {
        let (_db, d) = open().await;
        let admin = d.admin().unwrap();
        for result in [admin.create_schema("s").await, admin.rename_schema("s", "t").await, admin.drop_schema("s", false).await] {
            assert_eq!(result.err().unwrap(), "Schemas isn't supported by MySQL");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grants(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn global_grants_tell_what_an_account_may_do() {
        let all = global_privileges(&grants(&["GRANT ALL PRIVILEGES ON *.* TO `root`@`%` WITH GRANT OPTION", "GRANT PROXY ON ``@`` TO `root`@`%` WITH GRANT OPTION"]));
        assert_eq!(all, GlobalPrivileges { superuser: true, create_db: true, create_role: true, replication: true });

        let some = global_privileges(&grants(&["GRANT CREATE, CREATE USER, REPLICATION SLAVE ON *.* TO `u`@`%`"]));
        assert_eq!(some, GlobalPrivileges { superuser: false, create_db: true, create_role: true, replication: true });

        assert!(global_privileges(&grants(&["GRANT SELECT, SUPER ON *.* TO `u`@`%`"])).superuser);
        assert!(global_privileges(&grants(&["GRANT CREATE ROLE ON *.* TO `u`@`%`"])).create_role);
        // Other CREATEs are other privileges
        let views = global_privileges(&grants(&["GRANT CREATE VIEW, CREATE ROUTINE, CREATE TEMPORARY TABLES ON *.* TO `u`@`%`"]));
        assert_eq!(views, GlobalPrivileges::default());
        // Only what is granted on every database counts
        let scoped = global_privileges(&grants(&["GRANT USAGE ON *.* TO `u`@`%`", "GRANT ALL PRIVILEGES ON `shop`.* TO `u`@`%`", "GRANT SUPER ON `shop`.`t` TO `u`@`%`"]));
        assert_eq!(scoped, GlobalPrivileges::default());
        assert_eq!(global_privileges(&[]), GlobalPrivileges::default());
    }
}
