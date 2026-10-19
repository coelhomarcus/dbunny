//! The MariaDB driver: MariaDB's flavor and the way to open a session on a MariaDB server.
//! The session itself is `ServerDriver`.

mod flavor;
pub mod version;

use std::sync::Arc;

pub use flavor::MariadbFlavor;

use super::mysql_common::connect::MysqlEndpoint;
use super::mysql_common::driver::ServerDriver;

/// The entry point of the MariaDB engine.
pub struct MariadbDriver;

impl MariadbDriver {
    /// Connects to a MariaDB server (10.6 or newer) and opens a session on it.
    pub async fn open(endpoint: MysqlEndpoint, trust_host_key: Option<String>) -> Result<Arc<ServerDriver>, String> {
        ServerDriver::open(endpoint, trust_host_key, &MariadbFlavor).await
    }
}

/// Tests that need a MariaDB server: start them with `src-tauri/tests/mysql-docker.sh up`, then
/// run the suite with `DBUNNY_TEST_MYSQL_URL` set to `DBUNNY_TEST_MARIADB_URL` (11.4) and again
/// to `DBUNNY_TEST_MARIADB106_URL` (10.6). The refusals also need the MySQL server's url.
#[cfg(test)]
mod server_tests {
    use mysql_async::prelude::Queryable;

    use crate::db::mysql::MysqlDriver;
    use crate::db::mysql_common::tests_support::{endpoint_from, exec, is_mariadb, open_server, raw_conn, TestDb};
    use crate::db::{Driver, Engine};

    use super::MariadbDriver;

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL80_URL"]
    async fn mariadb_refuses_mysql() {
        let err = MariadbDriver::open(endpoint_from("DBUNNY_TEST_MYSQL80_URL"), None).await.err().unwrap();
        assert!(err.starts_with("This server is MySQL 8."), "{err}");
        assert!(err.ends_with(". Edit the connection and choose MySQL."), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MARIADB_URL"]
    async fn mysql_refuses_mariadb() {
        let err = MysqlDriver::open(endpoint_from("DBUNNY_TEST_MARIADB_URL"), None).await.err().unwrap();
        assert!(err.starts_with("This server is MariaDB 11.4."), "{err}");
        assert!(err.ends_with(". Edit the connection and choose MariaDB."), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn the_session_answers_as_mariadb() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        assert_eq!(driver.engine(), Engine::Mariadb);
        let version = driver.version();
        assert!(version.major >= 10);
        assert_eq!(driver.capabilities(), &crate::db::Capabilities::mariadb(version));
        let info = driver.info();
        assert_eq!(info.server_version, version.to_string());
        assert!(!info.server_version.contains('-'), "{}", info.server_version);
        assert_eq!(info.database, db.name);
        assert!(!info.read_only);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn a_mariadb_account_with_ed25519_connects() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        // The plugin is installed once per server; installing it again is an error to ignore
        let mut conn = raw_conn(&db.endpoint).await;
        let _ = conn.query_drop("INSTALL SONAME 'auth_ed25519'").await;
        let _ = conn.disconnect().await;
        let user = format!("ed_{}", &db.name[9..]);
        exec(&db.endpoint, &format!("CREATE USER '{user}'@'%' IDENTIFIED VIA ed25519 USING PASSWORD('secret')")).await;
        exec(&db.endpoint, &format!("GRANT SELECT ON `{}`.* TO '{user}'@'%'", db.name)).await;
        let endpoint = crate::db::mysql_common::connect::MysqlEndpoint { user: user.clone(), password: "secret".into(), ..db.endpoint.clone() };
        let opened = MariadbDriver::open(endpoint, None).await;
        exec(&db.endpoint, &format!("DROP USER '{user}'@'%'")).await;
        let driver = opened.expect("an ed25519 account logs in");
        assert_eq!(driver.engine(), Engine::Mariadb);
        driver.close().await;
        db.drop_now().await;
    }
}
