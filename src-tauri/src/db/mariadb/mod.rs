//! The MariaDB driver: MariaDB's flavor and the way to open a session on a MariaDB server.
//! The session itself is `ServerDriver`.

mod flavor;
pub(crate) mod plan;
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

    use crate::db::mysql_common::catalog::column_definitions;

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

    const COLUMNS: &str = r#"
        CREATE TABLE cols (
            id INT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            a INT DEFAULT 5,
            s VARCHAR(20) DEFAULT 'x',
            q VARCHAR(20) DEFAULT 'it''s \\ x',
            n INT NULL DEFAULT NULL,
            t TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
            d INT DEFAULT (1 + 1),
            u VARCHAR(36) DEFAULT (uuid()),
            g INT GENERATED ALWAYS AS (a * 2) STORED,
            v INT AS (a + 1) VIRTUAL,
            j JSON,
            CONSTRAINT ck_a CHECK (a < 100)
        )"#;

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_defaults_are_read_like_mysql() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(&db.endpoint, COLUMNS).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let mut conn = driver.conn().await.unwrap();
        let columns = column_definitions(&mut conn, driver.flavor(), &db.name, "cols").await.unwrap();
        let named = |name: &str| columns.iter().find(|c| c.name == name).unwrap();
        // Literals come without quotes, no default at all comes as None
        assert_eq!(named("a").default.as_deref(), Some("5"));
        assert_eq!(named("s").default.as_deref(), Some("x"));
        assert_eq!(named("q").default.as_deref(), Some("it's \\ x"));
        assert_eq!(named("n").default, None);
        assert_eq!(named("id").default, None);
        // Expressions are marked the way MySQL marks them
        for (name, default) in [("t", "current_timestamp()"), ("d", "(1 + 1)"), ("u", "uuid()")] {
            assert_eq!(named(name).default.as_deref(), Some(default), "{name}");
            assert!(named(name).extra.contains("DEFAULT_GENERATED"), "{name}: {}", named(name).extra);
        }
        assert!(named("t").extra.to_lowercase().contains("on update current_timestamp"), "{}", named("t").extra);
        assert!(!named("a").extra.contains("DEFAULT_GENERATED"));
        drop(conn);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_generated_columns_are_read() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(&db.endpoint, COLUMNS).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let columns = driver.catalog().columns(&db.name, "cols").await.unwrap();
        let named = |name: &str| columns.iter().find(|c| c.name == name).unwrap();
        assert!(named("g").is_generated && named("v").is_generated && !named("a").is_generated);
        let mut conn = driver.conn().await.unwrap();
        let definitions = column_definitions(&mut conn, driver.flavor(), &db.name, "cols").await.unwrap();
        let stored = definitions.iter().find(|c| c.name == "g").unwrap();
        assert!(stored.extra.contains("STORED GENERATED"), "{}", stored.extra);
        assert_eq!(stored.generation_expression.as_deref(), Some("`a` * 2"));
        drop(conn);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_json_column_keeps_its_check() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(&db.endpoint, COLUMNS).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let structure = driver.catalog().table_structure(&db.name, "cols").await.unwrap();
        // It reads as the JSON it was declared as, and its own check isn't a constraint to edit
        let json = structure.columns.iter().find(|c| c.name == "j").unwrap();
        assert_eq!(json.type_name, "json");
        assert!(structure.constraints.iter().all(|c| c.name != "j"), "{:?}", structure.constraints.iter().map(|c| &c.name).collect::<Vec<_>>());
        let check = structure.constraints.iter().find(|c| c.name == "ck_a").unwrap();
        assert_eq!((check.kind.as_str(), check.definition.as_str()), ("check", "CHECK (`a` < 100)"));

        // Changing another column of the table leaves the JSON column as it was
        let before: String = show_create(&db, "cols").await;
        let editor = driver.schema_editor().unwrap();
        let change = crate::db::model::StructureChange::SetComment { name: "j".into(), comment: None };
        editor.apply_change(&db.name, "cols", change).await.unwrap();
        assert_eq!(show_create(&db, "cols").await, before);
        // Dropping a check goes by CONSTRAINT on MariaDB
        let drop = crate::db::model::StructureChange::DropConstraint { name: "ck_a".into(), cascade: false };
        assert_eq!(editor.preview_change(&db.name, "cols", &drop).await.unwrap(), format!("ALTER TABLE `{}`.`cols` DROP CONSTRAINT `ck_a`;", db.name));
        editor.apply_change(&db.name, "cols", drop).await.unwrap();
        driver.close().await;
        db.drop_now().await;
    }

    async fn show_create(db: &TestDb, table: &str) -> String {
        let mut conn = raw_conn(&db.endpoint).await;
        let row: (String, String) = conn.query_first(format!("SHOW CREATE TABLE `{}`.`{table}`", db.name)).await.unwrap().unwrap();
        let _ = conn.disconnect().await;
        row.1
    }

    const PLAN_SHOP: &str = "
        CREATE TABLE customers (id INT PRIMARY KEY AUTO_INCREMENT, email VARCHAR(120) UNIQUE, name VARCHAR(80), age INT);
        CREATE TABLE orders (id INT PRIMARY KEY AUTO_INCREMENT, customer_id INT, total DECIMAL(10,2), KEY (customer_id));
        INSERT INTO customers (email, name, age) VALUES ('a@x', 'A', 1), ('b@x', 'B', 2), ('c@x', 'C', 3);
        INSERT INTO orders (customer_id, total) VALUES (1, 5), (1, 6), (2, 7);";

    /// The labels of every step of a plan, depth first.
    fn labels(nodes: &serde_json::Value) -> Vec<String> {
        nodes
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|node| std::iter::once(node["label"].as_str().unwrap().to_string()).chain(labels(&node["children"])))
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_plans_name_their_tables_and_analyze_measures_them() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(&db.endpoint, PLAN_SHOP).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let console = driver.console().unwrap();
        let sql = "SELECT c.name, SUM(o.total) FROM customers c JOIN orders o ON o.customer_id = c.id GROUP BY c.name ORDER BY 2 DESC";

        let plan = console.explain(sql, false).await.unwrap();
        assert_eq!((plan.format, plan.analyzed), (crate::db::ExplainFormat::MysqlTree, false));
        let steps = labels(&plan.plan);
        assert!(steps.iter().any(|l| l.contains("on c") || l.contains(" on c ")), "{steps:?}");
        assert!(steps.iter().any(|l| l.contains(" on o")), "{steps:?}");
        assert_eq!(plan.plan[0]["actualRows"], serde_json::json!(null));

        let analyzed = console.explain(sql, true).await.unwrap();
        assert!(analyzed.analyzed);
        let root = &analyzed.plan[0];
        assert!(root["actualRows"].is_number() && root["loops"].is_number(), "{root}");
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn explain_of_a_write_does_not_run_it() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(&db.endpoint, PLAN_SHOP).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let console = driver.console().unwrap();
        for sql in ["UPDATE customers SET name = 'Changed'", "DELETE FROM customers", "INSERT INTO customers (email) VALUES ('z@x')"] {
            let plan = console.explain(sql, false).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
            assert!(!plan.analyzed && plan.plan.as_array().is_some_and(|roots| !roots.is_empty()), "{sql}");
            let err = console.explain(sql, true).await.err().unwrap();
            assert_eq!(err, "EXPLAIN ANALYZE runs only reading statements on MariaDB.", "{sql}");
        }
        let (count, names): (i64, i64) = {
            let mut conn = raw_conn(&db.endpoint).await;
            let row = conn.query_first(format!("SELECT COUNT(*), SUM(name = 'Changed') FROM `{}`.customers", db.name)).await.unwrap().unwrap();
            let _ = conn.disconnect().await;
            row
        };
        assert_eq!((count, names), (3, 0), "nothing ran");
        driver.close().await;
        db.drop_now().await;
    }
}
