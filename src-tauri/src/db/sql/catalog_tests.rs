//! The editor's function catalogs (`web/src/lib/sql/dialects/catalog`), proven on the servers:
//! every example runs on every version the function is offered for.

use crate::db::version::{sql_features, ServerVersion};
use crate::db::Engine;

const POSTGRES: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/postgres.json");
const MYSQL: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mysql.json");
const MARIADB: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mariadb.json");
const SQLITE: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/sqlite.json");

/// The name and example of each function the editor offers on `engine` at `version`.
fn offered(catalog: &str, engine: Engine, version: Option<ServerVersion>) -> Vec<(String, String)> {
    let features = sql_features(engine, version);
    let entries: Vec<serde_json::Value> = serde_json::from_str(catalog).expect("the catalog is a json array");
    assert!(!entries.is_empty());
    entries
        .iter()
        .filter(|e| e["requires"].as_str().is_none_or(|feature| features.contains(&feature)))
        .map(|e| (e["name"].as_str().unwrap().to_string(), e["example"].as_str().unwrap().to_string()))
        .collect()
}

fn report(engine: &str, failures: Vec<String>) {
    assert!(failures.is_empty(), "{engine}: {} example(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn sqlite_catalog_runs() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut failures = Vec::new();
    for (name, example) in offered(SQLITE, Engine::Sqlite, None) {
        let run = || -> rusqlite::Result<()> {
            let mut statement = conn.prepare(&example)?;
            let mut rows = statement.query([])?;
            while rows.next()?.is_some() {}
            Ok(())
        };
        if let Err(e) = run() {
            failures.push(format!("{name}: {e}"));
        }
    }
    report("sqlite", failures);
}

mod postgres {
    use super::*;
    use crate::db::postgres::tests_support::{server_version, test_url};

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn postgres_catalog_runs() {
        let (client, connection) = tokio_postgres::connect(&test_url(), tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        let mut failures = Vec::new();
        for (name, example) in offered(POSTGRES, Engine::Postgres, Some(server_version().await)) {
            client.batch_execute("BEGIN").await.unwrap();
            if let Err(e) = client.simple_query(&example).await {
                failures.push(format!("{name}: {}", e.as_db_error().map_or(e.to_string(), |d| d.message().to_string())));
            }
            client.batch_execute("ROLLBACK").await.unwrap();
        }
        report("postgres", failures);
    }
}

mod mysql_common {
    use super::*;
    use crate::db::mysql_common::tests_support::{is_mariadb, raw_conn, test_endpoint, version};
    use mysql_async::prelude::Queryable;

    async fn run(catalog: &str, engine: Engine) {
        let mut conn = raw_conn(&test_endpoint()).await;
        let mut failures = Vec::new();
        for (name, example) in offered(catalog, engine, Some(version().await)) {
            if let Err(e) = conn.query_drop(&example).await {
                failures.push(format!("{name}: {e}"));
            }
        }
        let _ = conn.disconnect().await;
        report(if engine == Engine::Mariadb { "mariadb" } else { "mysql" }, failures);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mysql_catalog_runs() {
        if !is_mariadb().await {
            run(MYSQL, Engine::Mysql).await;
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_catalog_runs() {
        if is_mariadb().await {
            run(MARIADB, Engine::Mariadb).await;
        }
    }
}
