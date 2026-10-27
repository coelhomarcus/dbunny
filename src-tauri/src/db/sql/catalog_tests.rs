//! The editor's function catalogs and snippets (`web/src/lib/sql/dialects/catalog`), proven on the
//! servers: every example runs on every version the function or snippet is offered for. Snippets
//! run in a transaction that is rolled back, on a table `dbunny_snippet_t (id, name)`.

use crate::db::version::{sql_features, ServerVersion};
use crate::db::Engine;

const POSTGRES: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/postgres.json");
const MYSQL: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mysql.json");
const MARIADB: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mariadb.json");
const SQLITE: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/sqlite.json");
const POSTGRES_SNIPPETS: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/postgres.snippets.json");
const MYSQL_SNIPPETS: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mysql.snippets.json");
const MARIADB_SNIPPETS: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/mariadb.snippets.json");
const SQLITE_SNIPPETS: &str = include_str!("../../../../web/src/lib/sql/dialects/catalog/sqlite.snippets.json");

/// The name (or id, for a snippet) and example of each item the editor offers on `engine` at `version`.
fn offered(catalog: &str, engine: Engine, version: Option<ServerVersion>) -> Vec<(String, String)> {
    let features = sql_features(engine, version);
    let entries: Vec<serde_json::Value> = serde_json::from_str(catalog).expect("the catalog is a json array");
    assert!(!entries.is_empty());
    entries
        .iter()
        .filter(|e| e["requires"].as_str().is_none_or(|feature| features.contains(&feature)))
        .map(|e| {
            let name = e["name"].as_str().or(e["id"].as_str()).unwrap();
            (name.to_string(), e["example"].as_str().unwrap().to_string())
        })
        .collect()
}

fn report(engine: &str, failures: Vec<String>) {
    assert!(failures.is_empty(), "{engine}: {} example(s) failed:\n{}", failures.len(), failures.join("\n"));
}

/// Runs each example on `conn`; a snippet's runs in a transaction that is rolled back.
fn sqlite_runs(conn: &rusqlite::Connection, catalog: &str, rolled_back: bool) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, example) in offered(catalog, Engine::Sqlite, None) {
        let run = || -> rusqlite::Result<()> {
            let mut statement = conn.prepare(&example)?;
            let mut rows = statement.query([])?;
            while rows.next()?.is_some() {}
            Ok(())
        };
        if rolled_back {
            conn.execute_batch("BEGIN").unwrap();
        }
        if let Err(e) = run() {
            failures.push(format!("{name}: {e}"));
        }
        if rolled_back {
            conn.execute_batch("ROLLBACK").unwrap();
        }
    }
    failures
}

#[test]
fn sqlite_catalog_runs() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    report("sqlite", sqlite_runs(&conn, SQLITE, false));
}

#[test]
fn sqlite_snippets_run() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE dbunny_snippet_t (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
    report("sqlite snippets", sqlite_runs(&conn, SQLITE_SNIPPETS, true));
}

mod postgres {
    use super::*;
    use crate::db::postgres::tests_support::{server_version, test_url};

    /// Runs each example in a transaction that is rolled back.
    async fn run(client: &tokio_postgres::Client, catalog: &str) -> Vec<String> {
        let mut failures = Vec::new();
        for (name, example) in offered(catalog, Engine::Postgres, Some(server_version().await)) {
            client.batch_execute("BEGIN").await.unwrap();
            if let Err(e) = client.simple_query(&example).await {
                failures.push(format!("{name}: {}", e.as_db_error().map_or(e.to_string(), |d| d.message().to_string())));
            }
            client.batch_execute("ROLLBACK").await.unwrap();
        }
        failures
    }

    async fn connect() -> tokio_postgres::Client {
        let (client, connection) = tokio_postgres::connect(&test_url(), tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn postgres_catalog_runs() {
        report("postgres", run(&connect().await, POSTGRES).await);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn postgres_snippets_run() {
        let client = connect().await;
        // A temporary table: it goes with the session and comes first in the search path
        client.batch_execute("CREATE TEMP TABLE dbunny_snippet_t (id int PRIMARY KEY, name text)").await.unwrap();
        report("postgres snippets", run(&client, POSTGRES_SNIPPETS).await);
    }
}

mod mysql_common {
    use super::*;
    use crate::db::mysql_common::tests_support::{exec, is_mariadb, raw_conn, test_endpoint, version, TestDb};
    use crate::db::mysql_common::connect::MysqlEndpoint;
    use mysql_async::prelude::Queryable;

    /// Runs each example on `endpoint`; with `rolled_back`, in a transaction that is rolled back.
    async fn run(endpoint: &MysqlEndpoint, catalog: &str, engine: Engine, rolled_back: bool) -> Vec<String> {
        let mut conn = raw_conn(endpoint).await;
        let mut failures = Vec::new();
        for (name, example) in offered(catalog, engine, Some(version().await)) {
            if rolled_back {
                conn.query_drop("START TRANSACTION").await.unwrap();
            }
            if let Err(e) = conn.query_drop(&example).await {
                failures.push(format!("{name}: {e}"));
            }
            if rolled_back {
                conn.query_drop("ROLLBACK").await.unwrap();
            }
        }
        let _ = conn.disconnect().await;
        failures
    }

    const fn label(engine: Engine) -> &'static str {
        if matches!(engine, Engine::Mariadb) { "mariadb" } else { "mysql" }
    }

    /// The snippets, in a throwaway database: CREATE TABLE commits even inside a transaction.
    async fn snippets(catalog: &str, engine: Engine) {
        let db = TestDb::new().await;
        exec(&db.endpoint, "CREATE TABLE dbunny_snippet_t (id INT PRIMARY KEY, name VARCHAR(50))").await;
        let failures = run(&db.endpoint, catalog, engine, true).await;
        db.drop_now().await;
        report(&format!("{} snippets", label(engine)), failures);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mysql_catalog_runs() {
        if !is_mariadb().await {
            report("mysql", run(&test_endpoint(), MYSQL, Engine::Mysql, false).await);
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_catalog_runs() {
        if is_mariadb().await {
            report("mariadb", run(&test_endpoint(), MARIADB, Engine::Mariadb, false).await);
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mysql_snippets_run() {
        if !is_mariadb().await {
            snippets(MYSQL_SNIPPETS, Engine::Mysql).await;
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn mariadb_snippets_run() {
        if is_mariadb().await {
            snippets(MARIADB_SNIPPETS, Engine::Mariadb).await;
        }
    }
}
