//! The MySQL driver: MySQL's flavor and the way to open a session on a MySQL server. The
//! session itself is `ServerDriver`.

mod accounts;
mod flavor;
pub mod version;

use std::sync::Arc;

pub use flavor::MysqlFlavor;

use super::mysql_common::connect::MysqlEndpoint;
use super::mysql_common::driver::ServerDriver;

/// The entry point of the MySQL engine.
pub struct MysqlDriver;

impl MysqlDriver {
    /// Connects to a MySQL server (8.0 or newer) and opens a session on it.
    pub async fn open(endpoint: MysqlEndpoint, trust_host_key: Option<String>) -> Result<Arc<ServerDriver>, String> {
        ServerDriver::open(endpoint, trust_host_key, &MysqlFlavor).await
    }
}

#[cfg(test)]
mod tests {
    use crate::db::mysql_common::connect::{init_statements, ConnRole};

    #[tokio::test]
    async fn a_socket_cannot_be_reached_through_an_ssh_tunnel() {
        use crate::db::mysql_common::connect::MysqlEndpoint;
        use crate::db::net::ssh::SshConfig;
        let endpoint = MysqlEndpoint {
            host: "/var/run/mysqld/mysqld.sock".into(),
            ssh: Some(SshConfig { host: "bastion.invalid".into(), user: "me".into(), ..Default::default() }),
            known_hosts: Some(std::env::temp_dir().join("dbunny-never-read.json")),
            ..Default::default()
        };
        let err = super::MysqlDriver::open(endpoint, None).await.err().unwrap();
        assert_eq!(err, "A socket is a local file, so it can't be reached through an SSH tunnel");
    }

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

    use crate::db::mysql_common::tests_support::{exec, query_one, test_endpoint, TestDb, SERVER_SETTINGS};
    use crate::db::mysql_common::driver::ServerDriver;
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
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
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
        assert_eq!(super::version::parse_mysql_version(&full).unwrap(), version);

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
        // The other tests create and drop databases; this one needs the list to hold still
        let db = TestDb::alone().await;
        let driver = crate::db::mysql_common::tests_support::open_server(test_endpoint()).await.unwrap();
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
        let driver = crate::db::mysql_common::tests_support::open_server(endpoint.clone()).await.unwrap();
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
        let err = crate::db::mysql_common::tests_support::open_server(endpoint.clone()).await.err().unwrap();
        assert!(err.starts_with("Access denied for root@"), "{err}");
        assert_eq!(err, format!("Access denied for root@{}: check the user and password.", endpoint.host));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn an_unknown_database_says_so() {
        let endpoint = MysqlEndpoint { database: Some("nope_x".into()), ..test_endpoint() };
        let err = crate::db::mysql_common::tests_support::open_server(endpoint).await.err().unwrap();
        assert_eq!(err, "The database nope_x doesn't exist.");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_closed_port_says_it_couldnt_reach_the_server() {
        let endpoint = MysqlEndpoint { host: "127.0.0.1".into(), port: 1, ..test_endpoint() };
        let err = crate::db::mysql_common::tests_support::open_server(endpoint).await.err().unwrap();
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
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        assert!(driver.info().read_only);
        driver.close().await;
        drop(_restore);
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        assert!(!driver.info().read_only);
        driver.close().await;
        db.drop_now().await;
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
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
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
    async fn a_command_that_reconnects_while_the_session_closes_does_not_run() {
        let db = TestDb::new().await;
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        // The next command finds the connection lost and opens another
        let before = driver.editor().connection_id();
        exec(&test_endpoint(), &format!("KILL CONNECTION {before}")).await;
        let (reached, proceed) = (Arc::new(tokio::sync::Notify::new()), Arc::new(tokio::sync::Notify::new()));
        driver.editor().pause_next_run_for_test(Arc::clone(&reached), Arc::clone(&proceed));
        let marker = uuid::Uuid::new_v4().simple().to_string();
        let running = {
            let (driver, sql) = (Arc::clone(&driver), format!("SELECT SLEEP(30), '{marker}'"));
            tokio::spawn(async move {
                driver.editor().run(move |c| Box::pin(async move { c.query_drop(sql).await.map_err(|e| e.to_string()) })).await
            })
        };
        // close() lands after the command took the connection and before it reconnected
        reached.notified().await;
        tokio::time::timeout(Duration::from_secs(5), driver.close()).await.expect("close() must not hang");
        proceed.notify_one();

        let result = tokio::time::timeout(Duration::from_secs(5), running).await.expect("the command must not run its query");
        assert_eq!(result.unwrap().err().unwrap(), "Session not found");
        let sql = format!("SELECT COUNT(*) FROM information_schema.processlist WHERE info LIKE '%{marker}%' AND info NOT LIKE '%processlist%'");
        assert_eq!(query_one::<i64>(&test_endpoint(), &sql).await.unwrap(), 0, "the query must never reach the server");
        // The connection opened for it doesn't stay behind
        let reopened = driver.editor().connection_id();
        assert_ne!(reopened, before);
        let mut gone = false;
        for _ in 0..20 {
            if !connection_listed(reopened).await {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the connection {reopened} opened during close() is still on the server");
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_lost_editor_connection_comes_back_on_the_next_command() {
        let db = TestDb::new().await;
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
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
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
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

    // The TLS and SSH tests below need the MySQL 8.4 container: the certificates in
    // DBUNNY_TEST_MYSQL_SSL_DIR are its own, and the SSH target is its container name.

    fn ssl_file(name: &str) -> String {
        let dir = std::env::var("DBUNNY_TEST_MYSQL_SSL_DIR").expect("set DBUNNY_TEST_MYSQL_SSL_DIR to run the MySQL TLS tests");
        format!("{}/{name}", dir.trim_end_matches('/'))
    }

    fn with_tls(endpoint: MysqlEndpoint, mode: &str, root: &str, cert: &str, key: &str) -> MysqlEndpoint {
        let file = |name: &str| if name.is_empty() { String::new() } else { ssl_file(name) };
        MysqlEndpoint {
            tls: Some(crate::db::net::tls::TlsSettings { mode: mode.into(), root_cert: file(root), cert: file(cert), key: file(key) }),
            ..endpoint
        }
    }

    /// What `SHOW STATUS LIKE 'Ssl_cipher'` says on the editor's and on a pool connection.
    async fn ciphers(driver: &ServerDriver) -> (String, String) {
        let in_editor: (String, String) = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SHOW STATUS LIKE 'Ssl_cipher'").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        let in_pool: (String, String) = driver.conn().await.unwrap().query_first("SHOW STATUS LIKE 'Ssl_cipher'").await.unwrap().unwrap();
        (in_editor.1, in_pool.1)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn require_encrypts_with_the_servers_own_certificate() {
        let db = TestDb::new().await;
        // No CA file: the server's self-made certificate is accepted as it is
        let driver = MysqlDriver::open(with_tls(db.endpoint.clone(), "require", "", "", ""), None).await.unwrap();
        let (editor, pool) = ciphers(&driver).await;
        assert!(!editor.is_empty(), "the editor's connection isn't encrypted");
        assert!(!pool.is_empty(), "the pool's connection isn't encrypted");
        driver.close().await;

        // Without TLS the same server answers with no cipher
        let plain = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        assert_eq!(ciphers(&plain).await, (String::new(), String::new()));
        plain.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL (the 8.4 one) and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn verify_ca_checks_the_chain() {
        let db = TestDb::new().await;
        let name_message = "The server's certificate doesn't name 127.0.0.1, and verifying only the CA isn't possible yet. \
                            Use Required, or issue a certificate that names the host.";

        // The right CA vouches for the chain, but the server's certificate has no names.
        // Flip to Ok when mysql_async skips name errors again (see `ssl_opts`)
        let err = MysqlDriver::open(with_tls(db.endpoint.clone(), "verify-ca", "ca.pem", "", ""), None).await.err().unwrap();
        assert_eq!(err, name_message);

        // Another server's CA doesn't vouch for the chain: that is not the name problem
        let err = MysqlDriver::open(with_tls(db.endpoint.clone(), "verify-ca", "other-ca.pem", "", ""), None).await.err().unwrap();
        assert_ne!(err, name_message);
        assert!(err.contains("UnknownIssuer"), "{err}");

        // verify-full says it too, with its own text
        let err = MysqlDriver::open(with_tls(db.endpoint.clone(), "verify-full", "ca.pem", "", ""), None).await.err().unwrap();
        assert_eq!(err, "The server's certificate doesn't name 127.0.0.1.");
        db.drop_now().await;
    }

    /// A user that must log in with a client certificate; dropped with the value, also when
    /// the test fails.
    struct CertUser {
        name: String,
    }

    impl CertUser {
        async fn new() -> CertUser {
            let name = format!("dbunny_t_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            exec(&test_endpoint(), &format!("CREATE USER '{name}'@'%' IDENTIFIED BY 'certpass' REQUIRE X509")).await;
            CertUser { name }
        }
    }

    impl Drop for CertUser {
        fn drop(&mut self) {
            let sql = format!("DROP USER IF EXISTS '{}'@'%'", self.name);
            let _ = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                runtime.block_on(exec(&test_endpoint(), &sql));
            })
            .join();
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL (the 8.4 one) and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn client_certificates_log_in() {
        let user = CertUser::new().await;
        let as_user = MysqlEndpoint { user: user.name.clone(), password: "certpass".into(), ..test_endpoint() };

        // The account requires a certificate: without one the login is refused
        let err = MysqlDriver::open(with_tls(as_user.clone(), "require", "", "", ""), None).await.err().unwrap();
        assert!(!err.is_empty());

        let driver = crate::db::mysql_common::tests_support::open_server(with_tls(as_user, "require", "", "client-cert.pem", "client-key.pem")).await.unwrap();
        let who: String = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT CURRENT_USER()").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(who, format!("{}@%", user.name));
        let (editor, pool) = ciphers(&driver).await;
        assert!(!editor.is_empty() && !pool.is_empty());
        let subject: (String, String) = driver.conn().await.unwrap().query_first("SHOW STATUS LIKE 'Ssl_cipher'").await.unwrap().unwrap();
        assert!(!subject.1.is_empty());
        driver.close().await;
    }

    struct SshEnv {
        ssh: crate::db::net::ssh::SshConfig,
        target: (String, u16),
    }

    fn ssh_env() -> SshEnv {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run the MySQL SSH tests"));
        let ssh = url::Url::parse(&format!("ssh://{}", var("DBUNNY_TEST_MYSQL_SSH"))).unwrap();
        let target = var("DBUNNY_TEST_MYSQL_SSH_TARGET");
        let (host, port) = target.rsplit_once(':').unwrap();
        SshEnv {
            ssh: crate::db::net::ssh::SshConfig {
                host: ssh.host_str().unwrap().into(),
                port: ssh.port().unwrap_or(22),
                user: ssh.username().into(),
                auth_method: "password".into(),
                password: ssh.password().unwrap_or("").into(),
                ..Default::default()
            },
            target: (host.into(), port.parse().unwrap()),
        }
    }

    /// Opens a session through the SSH server, confirming its host key the way the UI does:
    /// the first attempt is refused with the fingerprint, the second passes it.
    async fn open_through_ssh(endpoint: MysqlEndpoint) -> Result<Arc<ServerDriver>, String> {
        let known_hosts = std::env::temp_dir().join(format!("dbunny-mysql-ssh-test-{}.json", uuid::Uuid::new_v4()));
        let endpoint = MysqlEndpoint { known_hosts: Some(known_hosts.clone()), ..endpoint };
        let err = match crate::db::mysql_common::tests_support::open_server(endpoint.clone()).await {
            Ok(_) => panic!("an unknown host key must not be accepted silently"),
            Err(e) => e,
        };
        let problem: serde_json::Value = serde_json::from_str(&err).unwrap_or_else(|_| panic!("not a host key error: {err}"));
        assert_eq!(problem["sshHostKey"]["status"], "unknown");
        let fingerprint = problem["sshHostKey"]["fingerprint"].as_str().unwrap().to_string();
        let result = MysqlDriver::open(endpoint, Some(fingerprint)).await;
        let _ = std::fs::remove_file(known_hosts);
        result
    }

    fn through_ssh(db: &TestDb) -> MysqlEndpoint {
        let env = ssh_env();
        MysqlEndpoint {
            // The container's name on the Docker network: only the SSH server can resolve it
            host: env.target.0,
            port: env.target.1,
            ssh: Some(env.ssh),
            ..db.endpoint.clone()
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_SSH and DBUNNY_TEST_MYSQL_SSH_TARGET"]
    async fn the_session_goes_through_an_ssh_tunnel() {
        let db = TestDb::new().await;
        let endpoint = through_ssh(&db);
        // Not reachable without the tunnel
        assert!(MysqlDriver::open(MysqlEndpoint { ssh: None, ..endpoint.clone() }, None).await.is_err());

        let driver = open_through_ssh(endpoint.clone()).await.unwrap();
        assert_eq!(driver.info().host, endpoint.host);
        assert_eq!(driver.info().database, db.name);
        let from_editor: i64 = driver
            .editor()
            .run(|c| Box::pin(async move { c.query_first("SELECT 1").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
            .await
            .unwrap();
        assert_eq!(from_editor, 1);
        let from_pool: i64 = driver.conn().await.unwrap().query_first("SELECT 1").await.unwrap().unwrap();
        assert_eq!(from_pool, 1);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_SSH, DBUNNY_TEST_MYSQL_SSH_TARGET and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn tls_works_through_an_ssh_tunnel() {
        let db = TestDb::new().await;
        // This tests the transport; the CA checks are in `verify_ca_checks_the_chain`
        let driver = open_through_ssh(with_tls(through_ssh(&db), "require", "", "", "")).await.unwrap();
        let (editor, pool) = ciphers(&driver).await;
        assert!(!editor.is_empty() && !pool.is_empty());
        driver.close().await;
        db.drop_now().await;
    }
}
