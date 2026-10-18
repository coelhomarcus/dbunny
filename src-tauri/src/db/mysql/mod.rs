//! The MySQL driver: a session on one database of a server, with a pool for the explorer,
//! table views, structure and administration, and the query editor's own connection.

mod editor;
#[cfg(test)]
mod tests_support;
pub mod version;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::Pool;

pub(crate) use editor::EditorConn;
use version::MysqlVersion;

use super::mysql_common::connect::{opts_for, ConnRole, MysqlEndpoint};
use super::mysql_common::errors::error_message;
use super::net::ssh::{open_tunnel, KnownHosts, Tunnel};
use super::{Capabilities, Catalog, ConnectionInfo, Data, Driver, Engine};

/// Databases every server has, never opened on their own.
const SYSTEM_DATABASES: [&str; 4] = ["mysql", "sys", "information_schema", "performance_schema"];

/// How long `close` waits for the editor's query to be killed.
const KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// One open session on a MySQL server.
pub struct MysqlDriver {
    /// The connections of the explorer, table views, structure and administration; None
    /// once the session was closed.
    pool: Mutex<Option<Pool>>,
    /// The query editor's connection, which also keeps its transaction.
    editor: EditorConn,
    /// Where the session connects, with the database it opened.
    endpoint: MysqlEndpoint,
    /// The SSH tunnel the connections go through; held to keep it open, and closed with
    /// the session.
    tunnel: Mutex<Option<Arc<Tunnel>>>,
    info: ConnectionInfo,
    #[allow(dead_code)] // read through `version()`
    version: MysqlVersion,
    capabilities: Capabilities,
}

impl MysqlDriver {
    /// Connects to `endpoint`: the tunnel, the editor's connection, the version check, the
    /// database to open when none was asked for, whether the server only reads, and the
    /// pool. `trust_host_key` is an SSH host key fingerprint the user just confirmed.
    pub async fn open(endpoint: MysqlEndpoint, trust_host_key: Option<String>) -> Result<Arc<MysqlDriver>, String> {
        let mut endpoint = endpoint;
        let tunnel = match (&endpoint.ssh, &endpoint.known_hosts) {
            (Some(ssh), Some(known_hosts)) => {
                let target = (endpoint.host.clone(), endpoint.port_or_default());
                Some(Arc::new(open_tunnel(ssh, target, KnownHosts::new(known_hosts.clone()), trust_host_key).await?))
            }
            _ => None,
        };
        let tunnel_port = tunnel.as_ref().map(|t| t.local_port);

        let editor = EditorConn::open(opts_for(&endpoint, tunnel_port, ConnRole::Editor)?, endpoint.clone()).await?;
        let shown = endpoint.clone();
        let fail = move |e: mysql_async::Error| error_message(&e, &shown);

        let fail_version = fail.clone();
        let text: String = editor
            .run(|c| Box::pin(async move { c.query_first("SELECT VERSION()").await.map(Option::unwrap_or_default).map_err(fail_version) }))
            .await?;
        let version = MysqlVersion::parse(&text)?;

        if endpoint.database.as_deref().unwrap_or("").is_empty() {
            let fail_first = fail.clone();
            let first: Option<String> = editor
                .run(|c| {
                    Box::pin(async move {
                        c.exec_first(
                            "SELECT schema_name FROM information_schema.schemata \
                             WHERE schema_name NOT IN (?, ?, ?, ?) ORDER BY schema_name LIMIT 1",
                            SYSTEM_DATABASES.to_vec(),
                        )
                        .await
                        .map_err(fail_first)
                    })
                })
                .await?;
            endpoint.database = first;
            if let Some(name) = endpoint.database.clone() {
                let fail_use = fail.clone();
                let sql = format!("USE `{}`", name.replace('`', "``"));
                editor.run(|c| Box::pin(async move { c.query_drop(sql).await.map_err(fail_use) })).await?;
                editor.set_opts(opts_for(&endpoint, tunnel_port, ConnRole::Editor)?);
            }
        }

        let fail_flags = fail.clone();
        let (server_read_only, super_read_only): (i64, i64) = editor
            .run(|c| {
                Box::pin(async move {
                    c.query_first("SELECT @@read_only, @@super_read_only").await.map(Option::unwrap_or_default).map_err(fail_flags)
                })
            })
            .await?;

        let pool = Pool::new(opts_for(&endpoint, tunnel_port, ConnRole::Pool)?);
        let info = ConnectionInfo {
            database: endpoint.database.clone().unwrap_or_default(),
            host: endpoint.host.clone(),
            port: endpoint.port_or_default(),
            server_version: format!("{}.{}.{}", version.major, version.minor, version.patch),
            read_only: endpoint.read_only || server_read_only != 0 || super_read_only != 0,
        };
        Ok(Arc::new(MysqlDriver {
            pool: Mutex::new(Some(pool)),
            editor,
            endpoint,
            tunnel: Mutex::new(tunnel),
            info,
            version,
            capabilities: Capabilities::mysql(version),
        }))
    }

    /// A connection from the pool; it goes back when dropped.
    #[allow(dead_code)] // used by the catalog and the data (Tasks 6 and 7)
    pub(crate) async fn conn(&self) -> Result<mysql_async::Conn, String> {
        let pool = lock(&self.pool).clone().ok_or_else(|| "Session not found".to_string())?;
        pool.get_conn().await.map_err(|e| error_message(&e, &self.endpoint))
    }

    /// The query editor's connection.
    #[allow(dead_code)] // used by the console and the transactions (Tasks 9 and 10)
    pub(crate) fn editor(&self) -> &EditorConn {
        &self.editor
    }

    /// The database the session opened, or "" when the server had none to open.
    #[allow(dead_code)] // used by the catalog (Task 6)
    pub(crate) fn database(&self) -> &str {
        self.endpoint.database.as_deref().unwrap_or("")
    }

    /// Where the session connects, with the database it opened.
    #[allow(dead_code)] // used to open the other databases (Task 13)
    pub fn endpoint(&self) -> &MysqlEndpoint {
        &self.endpoint
    }

    /// The server's version, for what changes with it (CHECK constraints, EXPLAIN ANALYZE).
    #[allow(dead_code)] // used by the console and the structure (Tasks 9 and 11)
    pub fn version(&self) -> MysqlVersion {
        self.version
    }
}

#[async_trait]
impl Driver for MysqlDriver {
    fn engine(&self) -> Engine {
        Engine::Mysql
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn info(&self) -> ConnectionInfo {
        self.info.clone()
    }

    /// Lets go of the editor's connection and kills it on the server, so a query it is
    /// running stops there too (waiting at most 2 s for that), then shuts the pool down and
    /// closes the tunnel. Anything the session is asked afterwards answers "Session not found".
    async fn close(&self) {
        let editor_id = self.editor.connection_id();
        self.editor.close();
        let pool = lock(&self.pool).take();
        if let Some(pool) = pool {
            let kill = async {
                let mut conn = pool.get_conn().await?;
                conn.query_drop(format!("KILL CONNECTION {editor_id}")).await
            };
            // The best it can do: the editor may already be gone, or the server unreachable
            let _ = tokio::time::timeout(KILL_TIMEOUT, kill).await;
            // Waits for the connections commands still hold, so it isn't waited for here
            tokio::spawn(async move {
                let _ = pool.disconnect().await;
            });
        }
        let tunnel = lock(&self.tunnel).take();
        drop(tunnel);
    }

    fn catalog(&self) -> &dyn Catalog {
        unimplemented!("the MySQL catalog comes with Task 6")
    }

    fn data(&self) -> &dyn Data {
        unimplemented!("MySQL table data comes with Task 7")
    }
}

/// A mutex a panicking task left poisoned still guards a usable value.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use crate::db::mysql_common::connect::{init_statements, ConnRole};

    #[test]
    fn init_statements_follow_read_only_and_role() {
        assert_eq!(init_statements(false, ConnRole::Editor), vec!["SET NAMES utf8mb4", "SET time_zone = '+00:00'"]);
        let pool = init_statements(true, ConnRole::Pool);
        assert!(pool.contains(&"SET SESSION TRANSACTION READ ONLY".to_string()));
        assert!(pool.contains(&"SET SESSION innodb_lock_wait_timeout = 5".to_string()));
    }
}

/// Tests that need a MySQL server: start one with `src-tauri/tests/mysql-docker.sh up`, then
///   cargo test --manifest-path src-tauri/Cargo.toml --lib mysql -- --ignored
/// with the variables it prints (and once more with DBUNNY_TEST_MYSQL_URL set to the 8.0 one).
#[cfg(test)]
mod server_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use mysql_async::prelude::Queryable;

    use super::tests_support::{endpoint_from, exec, query_one, test_endpoint, TestDb, SERVER_SETTINGS};
    use super::version::MysqlVersion;
    use super::*;
    use crate::db::mysql_common::connect::MysqlEndpoint;
    use crate::db::mysql_common::errors::error_message;
    use crate::db::{Capabilities, Driver, Engine};

    const SYSTEM: [&str; 4] = ["mysql", "sys", "information_schema", "performance_schema"];

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn the_session_answers_as_mysql() {
        let _settings = SERVER_SETTINGS.read().await;
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        assert_eq!(driver.engine(), Engine::Mysql);
        let version = driver.version();
        assert!(version.major >= 8);
        assert_eq!(driver.capabilities(), &Capabilities::mysql(version));
        let info = driver.info();
        assert_eq!(info.server_version, format!("{}.{}.{}", version.major, version.minor, version.patch));
        assert!(!info.server_version.contains('-'), "{}", info.server_version);
        assert_eq!(info.database, db.name);
        assert_eq!(driver.database(), db.name);
        assert_eq!(info.host, db.endpoint.host);
        assert_eq!(info.port, db.endpoint.port);
        assert!(!info.read_only);
        let full: String = query_one(&db.endpoint, "SELECT VERSION()").await.unwrap();
        assert_eq!(MysqlVersion::parse(&full).unwrap(), version);

        // Both kinds of connection are in the database, with the session settings
        let editor: (String, String) = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT DATABASE(), @@session.time_zone").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(editor, (db.name.clone(), "+00:00".to_string()));
        let mut pool = driver.conn().await.unwrap();
        let pooled: (String, String, u32) =
            pool.query_first("SELECT DATABASE(), @@session.time_zone, @@session.innodb_lock_wait_timeout").await.unwrap().unwrap();
        assert_eq!(pooled, (db.name.clone(), "+00:00".to_string(), 5));
        drop(pool);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn without_a_database_the_first_user_database_opens() {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(test_endpoint(), None).await.unwrap();
        let opened = driver.info().database;
        assert!(!opened.is_empty() && !SYSTEM.contains(&opened.as_str()), "{opened}");
        let first: String = query_one(
            &test_endpoint(),
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('mysql', 'sys', 'information_schema', 'performance_schema') \
             ORDER BY schema_name LIMIT 1",
        )
        .await
        .unwrap();
        assert_eq!(opened, first);
        assert_eq!(driver.database(), opened);
        let in_editor: Option<String> = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT DATABASE()").await.map(Option::flatten).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(in_editor.as_deref(), Some(opened.as_str()));
        let in_pool: Option<String> = driver.conn().await.unwrap().query_first("SELECT DATABASE()").await.unwrap().flatten();
        assert_eq!(in_pool.as_deref(), Some(opened.as_str()));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn read_only_refuses_writes_and_ddl() {
        let db = TestDb::new().await;
        exec(&db.endpoint, "CREATE TABLE t (x INT)").await;
        let endpoint = MysqlEndpoint { read_only: true, ..db.endpoint.clone() };
        let driver = MysqlDriver::open(endpoint.clone(), None).await.unwrap();
        assert!(driver.info().read_only);
        for sql in ["INSERT INTO t VALUES (1)", "CREATE TABLE u (x INT)"] {
            let shown = endpoint.clone();
            let err = driver
                .editor()
                .run(move |c| Box::pin(async move { c.query_drop(sql).await.map_err(|e| error_message(&e, &shown)) }))
                .await
                .err()
                .unwrap();
            assert_eq!(err, "This connection is read-only.", "editor: {sql}");
            let err = driver.conn().await.unwrap().query_drop(sql).await.err().unwrap();
            assert_eq!(error_message(&err, &endpoint), "This connection is read-only.", "pool: {sql}");
        }
        // Reading still works
        let count: i64 = driver.conn().await.unwrap().query_first("SELECT COUNT(*) FROM t").await.unwrap().unwrap();
        assert_eq!(count, 0);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn wrong_password_says_so() {
        let endpoint = MysqlEndpoint { password: "certainly-wrong".into(), ..test_endpoint() };
        let err = MysqlDriver::open(endpoint.clone(), None).await.err().unwrap();
        assert!(err.starts_with("Access denied for root@"), "{err}");
        assert_eq!(err, format!("Access denied for root@{}: check the user and password.", endpoint.host));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn an_unknown_database_says_so() {
        let endpoint = MysqlEndpoint { database: Some("nope_x".into()), ..test_endpoint() };
        let err = MysqlDriver::open(endpoint, None).await.err().unwrap();
        assert_eq!(err, "The database nope_x doesn't exist.");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_closed_port_says_it_couldnt_reach_the_server() {
        let endpoint = MysqlEndpoint { host: "127.0.0.1".into(), port: 1, ..test_endpoint() };
        let err = MysqlDriver::open(endpoint, None).await.err().unwrap();
        assert!(err.starts_with("Couldn't reach 127.0.0.1:1"), "{err}");
    }

    /// Turns the server's `read_only` back off when dropped, also when the test fails.
    struct WritableAgain;
    impl Drop for WritableAgain {
        fn drop(&mut self) {
            let _ = std::thread::spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                runtime.block_on(exec(&test_endpoint(), "SET GLOBAL read_only = OFF"));
            })
            .join();
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_read_only_server_marks_the_session() {
        let _settings = SERVER_SETTINGS.write().await;
        let db = TestDb::new().await;
        // Only read_only, not super_read_only: root (and so the other tests) can still write
        exec(&test_endpoint(), "SET GLOBAL read_only = ON").await;
        let _restore = WritableAgain;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        assert!(driver.info().read_only);
        driver.close().await;
        drop(_restore);
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        assert!(!driver.info().read_only);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MARIADB_URL"]
    async fn mariadb_servers_are_refused() {
        let err = MysqlDriver::open(endpoint_from("DBUNNY_TEST_MARIADB_URL"), None).await.err().unwrap();
        assert!(err.starts_with("This server is MariaDB 11.4."), "{err}");
        assert!(err.ends_with(", which DBunny doesn't support yet."), "{err}");
    }

    /// Whether the server still lists connection `id`.
    async fn connection_listed(id: u32) -> bool {
        let sql = format!("SELECT COUNT(*) FROM information_schema.processlist WHERE id = {id}");
        query_one::<i64>(&test_endpoint(), &sql).await.unwrap() > 0
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn closing_during_a_running_query_does_not_hang() {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        let id = driver.editor().connection_id();
        assert!(id > 0 && connection_listed(id).await);
        let running = {
            let driver = Arc::clone(&driver);
            tokio::spawn(async move {
                driver
                    .editor()
                    .run(|c| Box::pin(async move { c.query_drop("SELECT SLEEP(30)").await.map_err(|e| e.to_string()) }))
                    .await
            })
        };
        // Let the query reach the server
        tokio::time::sleep(Duration::from_millis(300)).await;

        let closing = tokio::time::timeout(Duration::from_secs(5), driver.close()).await;
        assert!(closing.is_ok(), "close() must not wait for the running query");
        let finished = tokio::time::timeout(Duration::from_secs(5), running).await;
        assert!(finished.is_ok(), "the running query must end within 5s of close()");
        let mut gone = false;
        for _ in 0..20 {
            if !connection_listed(id).await {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the editor's connection {id} is still on the server");

        // A command that still holds the driver is told the session is gone
        let err = driver.editor().run(|_| Box::pin(async { Ok(()) })).await.err().unwrap();
        assert_eq!(err, "Session not found");
        assert_eq!(driver.conn().await.err().unwrap(), "Session not found");
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_lost_editor_connection_comes_back_on_the_next_command() {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        let before = driver.editor().connection_id();
        exec(&test_endpoint(), &format!("KILL CONNECTION {before}")).await;

        let one: i64 = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT 1").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(one, 1);
        let after = driver.editor().connection_id();
        assert_ne!(before, after, "a new connection replaces the killed one");
        // The new connection is in the same database, with the same settings
        let state: (String, String) = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT DATABASE(), @@session.time_zone").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(state, (db.name.clone(), "+00:00".to_string()));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_connection_lost_inside_a_transaction_is_reported_once() {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        driver.editor().run(|c| Box::pin(async move { c.query_drop("START TRANSACTION").await.map_err(|e| e.to_string()) })).await.unwrap();
        // What the console notes after each statement (Task 10)
        driver.editor().set_in_transaction_for_test(true);
        exec(&test_endpoint(), &format!("KILL CONNECTION {}", driver.editor().connection_id())).await;

        let err = driver.editor().run(|_| Box::pin(async { Ok(()) })).await.err().unwrap();
        assert_eq!(err, "The connection was lost; the open transaction was rolled back");
        assert!(!driver.editor().in_transaction());
        let one: i64 = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT 1").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(one, 1);
        driver.close().await;
        db.drop_now().await;
    }
}
