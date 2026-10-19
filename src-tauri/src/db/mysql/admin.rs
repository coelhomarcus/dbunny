use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Error as MysqlError, Row};

use super::catalog::{number, quote, string, text};
use super::MysqlDriver;
use crate::db::model::{DatabaseInfo, RoleInfo, ServerActivity, ServerSession};
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;
use crate::db::{unsupported, Driver, Engine, ServerAdmin};

/// Databases every server has; they are never listed.
const SYSTEM_DATABASES: &str = "'mysql', 'sys', 'information_schema', 'performance_schema'";

/// How long dropping a database waits for the locks other sessions hold. The server's own
/// wait is a year.
const DROP_LOCK_WAIT: Duration = Duration::from_secs(10);

/// The server answers a `KILL` for a session that is already gone with this error.
const UNKNOWN_THREAD: u16 = 1094;

/// Waiting for a lock longer than `lock_wait_timeout` allows.
const LOCK_WAIT_TIMEOUT: u16 = 1205;

/// The errors for a table or a privilege the account lacks (1142: no SELECT, 1227: no PROCESS).
const DENIED: [u16; 3] = [1142, 1143, 1227];

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

    async fn roles(&self) -> Result<Vec<RoleInfo>, String> {
        unimplemented!("accounts and roles come in Task 14")
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
