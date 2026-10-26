//! The MariaDB driver: MariaDB's flavor and the way to open a session on a MariaDB server.
//! The session itself is `ServerDriver`.

mod accounts;
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

/// Tests that need a MariaDB server: start one with `src-tauri/tests/servers.sh up mariadb <version>`
/// (it also starts the MySQL server the refusals need), or run every version with `matrix.sh`.
#[cfg(test)]
mod server_tests {
    use mysql_async::prelude::Queryable;

    use crate::db::mysql::MysqlDriver;
    use crate::db::mysql_common::tests_support::{
        ciphers, endpoint_from, exec, is_mariadb, open_server, open_through_ssh, raw_conn, test_endpoint, through_ssh, version, with_tls, CertUser, TestDb,
        SERVER_SETTINGS,
    };
    use crate::db::{Driver, Engine};

    use crate::db::mysql_common::catalog::column_definitions;

    use super::MariadbDriver;

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL80_URL"]
    async fn mariadb_refuses_mysql() {
        let err = MariadbDriver::open(endpoint_from("DBUNNY_TEST_MYSQL80_URL"), None).await.err().unwrap();
        assert!(err.starts_with("This server is MySQL "), "{err}");
        assert!(err.ends_with(". Edit the connection and choose MySQL."), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MARIADB_URL"]
    async fn mysql_refuses_mariadb() {
        let err = MysqlDriver::open(endpoint_from("DBUNNY_TEST_MARIADB_URL"), None).await.err().unwrap();
        assert!(err.starts_with("This server is MariaDB "), "{err}");
        assert!(err.ends_with(". Edit the connection and choose MariaDB."), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn the_session_answers_as_mariadb() {
        if !is_mariadb().await {
            return;
        }
        // It checks the session isn't read-only, which a_read_only_mariadb_server_marks_the_session changes
        let _settings = SERVER_SETTINGS.read().await;
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

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn a_locked_mariadb_account_cannot_log_in() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let (locked, open_one) = (format!("dbunny_t_{}", &db.name[9..]), format!("dbunny_t_o{}", &db.name[9..]));
        exec(&db.endpoint, &format!("CREATE USER '{locked}'@'%' ACCOUNT LOCK; CREATE USER '{open_one}'@'%'")).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let list = driver.admin().unwrap().roles().await.unwrap();
        exec(&db.endpoint, &format!("DROP USER '{locked}'@'%'; DROP USER '{open_one}'@'%'")).await;
        let find = |name: &str| list.roles.iter().find(|r| r.name == name).unwrap();
        assert!(!find(&locked).can_login, "a locked account can't log in");
        assert!(find(&open_one).can_login);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn activity_lists_sessions_without_performance_schema() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let off: Option<i64> = crate::db::mysql_common::tests_support::query_one(&db.endpoint, "SELECT @@performance_schema").await;
        assert_eq!(off, Some(0), "the test server runs without the performance schema, like MariaDB by default");
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let activity = driver.admin().unwrap().activity(true).await.unwrap();
        assert!(!activity.limited_view);
        assert!(activity.sessions.iter().any(|s| s.is_self), "DBunny's own session is listed");
        assert!(activity.sessions.iter().all(|s| s.application.is_none()));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn table_privileges_are_granted_to_and_revoked_from_a_role() {
        use crate::db::model::StructureChange;
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let role = format!("dbunny_t_r{}", &db.name[9..]);
        exec(&db.endpoint, &format!("CREATE TABLE t (id INT); CREATE ROLE {role}")).await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let editor = driver.schema_editor().unwrap();
        let select = vec!["SELECT".to_string()];

        let granted = editor.apply_change(&db.name, "t", StructureChange::Grant { grantee: role.clone(), privileges: select.clone(), grant_option: false }).await;
        let listed = driver.catalog().table_structure(&db.name, "t").await.map(|s| s.privileges);
        let revoked = match &listed {
            // Revoked by the name the Permissions tab shows for it
            Ok(privileges) => match privileges.iter().find(|p| p.grantee.starts_with(&role)) {
                Some(entry) => editor.apply_change(&db.name, "t", StructureChange::Revoke { grantee: entry.grantee.clone(), privileges: select.clone() }).await,
                None => Err("the role isn't listed".to_string()),
            },
            Err(e) => Err(e.clone()),
        };
        exec(&db.endpoint, &format!("DROP ROLE {role}")).await;
        granted.unwrap();
        revoked.unwrap();
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn a_system_versioned_table_is_a_table_like_any_other() {
        use crate::db::model::TableDataParams;
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        exec(
            &db.endpoint,
            "CREATE TABLE parent (id INT PRIMARY KEY); \
             CREATE TABLE hist (id INT PRIMARY KEY, parent_id INT, v INT, FOREIGN KEY (parent_id) REFERENCES parent (id)) WITH SYSTEM VERSIONING; \
             INSERT INTO parent VALUES (1); INSERT INTO hist (id, parent_id, v) VALUES (1, 1, 10)",
        )
        .await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let catalog = driver.catalog();
        let tree = catalog.explorer().await.unwrap();
        assert!(tree.schemas[0].objects.iter().any(|o| o.name == "hist" && o.kind == "table"), "the explorer lists it");
        assert_eq!(catalog.relation_kind(&db.name, "hist").await.unwrap().as_deref(), Some("table"));
        let structure = catalog.table_structure(&db.name, "hist").await.unwrap();
        assert_eq!(structure.meta.kind, "table");
        assert!(structure.ddl.contains("WITH SYSTEM VERSIONING"), "{}", structure.ddl);
        let definition = catalog.object_definition(&db.name, "hist", "table").await.unwrap();
        assert!(definition.ddl.contains("WITH SYSTEM VERSIONING"));
        let completion = catalog.completion_schema().await.unwrap();
        assert!(completion.relations.iter().any(|r| r.name == "hist"));
        let diagram = catalog.diagram(&db.name).await.unwrap();
        assert!(diagram.tables.iter().any(|t| t.name == "hist"));
        let page = driver.data().table_page(&db.name, "hist", TableDataParams::default()).await.unwrap();
        assert_eq!(page.total_rows, 1);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn mariadb_require_encrypts_every_connection() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let driver = open_server(with_tls(db.endpoint.clone(), "require", "", "", "")).await.unwrap();
        let (editor, pool) = ciphers(&driver).await;
        assert!(!editor.is_empty(), "the editor's connection isn't encrypted");
        assert!(!pool.is_empty(), "the pool's connection isn't encrypted");
        driver.close().await;

        // Without TLS the same server answers with no cipher
        let plain = open_server(db.endpoint.clone()).await.unwrap();
        assert_eq!(ciphers(&plain).await, (String::new(), String::new()));
        plain.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn mariadb_verify_full_accepts_a_certificate_that_names_the_host() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        // servers.sh signs the MariaDB certificate for localhost and 127.0.0.1 with ca.pem
        let full = open_server(with_tls(db.endpoint.clone(), "verify-full", "ca.pem", "", "")).await.unwrap();
        full.close().await;
        // verify-ca checks the name too (a limit of the TLS library), and the name is right here
        let ca = open_server(with_tls(db.endpoint.clone(), "verify-ca", "ca.pem", "", "")).await.unwrap();
        ca.close().await;
        // A CA that didn't sign the certificate doesn't vouch for it
        let err = open_server(with_tls(db.endpoint.clone(), "verify-ca", "other-ca.pem", "", "")).await.err().unwrap();
        assert!(err.contains("UnknownIssuer"), "{err}");
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL and DBUNNY_TEST_MYSQL_SSL_DIR"]
    async fn mariadb_client_certificates_log_in() {
        if !is_mariadb().await {
            return;
        }
        let user = CertUser::new().await;
        let as_user = crate::db::mysql_common::connect::MysqlEndpoint { user: user.name.clone(), password: "certpass".into(), ..test_endpoint() };

        // The account requires a certificate: without one the login is refused
        assert!(open_server(with_tls(as_user.clone(), "require", "", "", "")).await.is_err());

        let driver = open_server(with_tls(as_user, "require", "", "client-cert.pem", "client-key.pem")).await.unwrap();
        let who: String = driver.conn().await.unwrap().query_first("SELECT CURRENT_USER()").await.unwrap().unwrap();
        assert_eq!(who, format!("{}@%", user.name));
        driver.close().await;
    }

    #[tokio::test]
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL, DBUNNY_TEST_MYSQL_SSH and DBUNNY_TEST_MYSQL_SSH_TARGET"]
    async fn mariadb_session_goes_through_an_ssh_tunnel() {
        if !is_mariadb().await {
            return;
        }
        let db = TestDb::new().await;
        let endpoint = through_ssh(&db);
        let driver = open_through_ssh(endpoint.clone()).await.unwrap();
        assert_eq!(driver.info().database, db.name);
        let one: i64 = driver.conn().await.unwrap().query_first("SELECT 1").await.unwrap().unwrap();
        assert_eq!(one, 1);
        driver.close().await;

        // TLS inside the tunnel
        let secure = open_through_ssh(with_tls(endpoint, "require", "", "", "")).await.unwrap();
        let (editor, pool) = ciphers(&secure).await;
        assert!(!editor.is_empty() && !pool.is_empty());
        secure.close().await;
        db.drop_now().await;
    }

    /// Turns the server's read_only off again when dropped, also when the test fails.
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
    #[ignore = "needs a MariaDB server in DBUNNY_TEST_MYSQL_URL"]
    async fn a_read_only_mariadb_server_marks_the_session() {
        if !is_mariadb().await {
            return;
        }
        let _settings = SERVER_SETTINGS.write().await;
        let db = TestDb::new().await;
        let _restore = WritableAgain;
        // From 12.x read_only is OFF, ON, NO_LOCK or NO_LOCK_NO_ADMIN instead of 0 or 1. Not
        // NO_LOCK_NO_ADMIN: it would stop root, and so the tests running alongside, from writing
        let modes: &[&str] = if version().await.at_least(12, 0, 0) { &["ON", "NO_LOCK"] } else { &["ON"] };
        for mode in modes {
            exec(&test_endpoint(), &format!("SET GLOBAL read_only = {mode}")).await;
            let driver = open_server(db.endpoint.clone()).await.unwrap();
            assert!(driver.info().read_only, "read_only = {mode}");
            driver.close().await;
        }
        exec(&test_endpoint(), "SET GLOBAL read_only = OFF").await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        assert!(!driver.info().read_only);
        driver.close().await;
        db.drop_now().await;
    }
}
