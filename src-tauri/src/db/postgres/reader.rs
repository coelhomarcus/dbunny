//! Reads for the assistant: one statement that only reads, in a read-only transaction that is
//! always rolled back, on a connection of its own.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::Type;
use tokio_postgres::{Client, SimpleQueryMessage};

use super::connect::open_connection;
use super::console::run_statement;
use super::data::{columns_from_statement, db_message, row_to_json_values};
use super::text::{is_native, text_to_json};
use super::PostgresDriver;
use crate::db::model::{ReadLimits, ReadResult};
use crate::db::sql::reading::{check_reading, TIMED_OUT};
use crate::db::sql::split::split_statements;
use crate::db::ReadOnlyQueries;

/// How long past the statement's own timeout the client waits before giving up on the connection.
const CLIENT_GRACE: Duration = Duration::from_secs(2);
const CURSOR: &str = "dbunny_read";

impl PostgresDriver {
    /// Runs `sql` on the assistant's connection without asking the classifier first. The
    /// read-only transaction refuses to write, which the tests rely on.
    pub(super) async fn read_unchecked(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        // One read at a time: what runs on the connection stays inside its own transaction
        let _reading = self.reading.lock().await;
        let client = self.reader_client().await?;
        match tokio::time::timeout(limits.timeout + CLIENT_GRACE, read_in_transaction(&client, sql, limits)).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => {
                // A connection that can't roll back is not trusted again
                if client.is_closed() || client.simple_query("SELECT 1").await.is_err() {
                    self.forget_reader().await;
                }
                Err(e)
            }
            Err(_) => {
                if let Ok(link) = self.link().await {
                    let _ = client.cancel_token().cancel_query(link.tls).await;
                }
                self.forget_reader().await;
                Err(TIMED_OUT.to_string())
            }
        }
    }

    /// The assistant's connection, opened (or opened again after it was lost) here.
    async fn reader_client(&self) -> Result<Arc<Client>, String> {
        {
            let state = self.lock().await;
            let session = state.as_ref().ok_or_else(|| "Session not found".to_string())?;
            if let Some(client) = session.reader.as_ref().filter(|c| !c.is_closed()) {
                return Ok(Arc::clone(client));
            }
        }
        // Reconnecting the main connection first also refreshes the link (a new SSH tunnel port)
        self.client().await?;
        let (link, notices) = (self.link().await?, self.notices().await?);
        let client = Arc::new(
            open_connection(&link, &notices).await.map_err(|e| format!("Couldn't open the assistant's connection: {e}"))?,
        );
        let mut state = self.lock().await;
        let session = state.as_mut().ok_or_else(|| "Session not found".to_string())?;
        session.reader = Some(Arc::clone(&client));
        Ok(client)
    }

    async fn forget_reader(&self) {
        if let Some(session) = self.lock().await.as_mut() {
            session.reader = None;
        }
    }
}

#[async_trait]
impl ReadOnlyQueries for PostgresDriver {
    async fn query(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        check_reading(crate::db::sql::Dialect::Postgres, sql).map_err(|refusal| refusal.message().to_string())?;
        self.read_unchecked(sql, limits).await
    }
}

/// BEGIN READ ONLY, the statement, ROLLBACK.
async fn read_in_transaction(client: &Client, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
    client.batch_execute("BEGIN READ ONLY").await.map_err(|e| db_message(&e))?;
    let outcome = read(client, sql, limits).await;
    let rolled_back = client.batch_execute("ROLLBACK").await;
    let result = outcome?;
    rolled_back.map_err(|e| db_message(&e))?;
    Ok(result)
}

async fn read(client: &Client, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
    let failed = |e: tokio_postgres::Error| {
        if e.code() == Some(&SqlState::QUERY_CANCELED) {
            TIMED_OUT.to_string()
        } else {
            db_message(&e)
        }
    };
    client.batch_execute(&format!("SET LOCAL statement_timeout = {}", limits.timeout.as_millis())).await.map_err(failed)?;
    let wanted = limits.rows + 1;

    // SHOW can't be a cursor; it returns a handful of rows
    let first = sql.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    if first.eq_ignore_ascii_case("show") {
        let output = run_statement(client, sql).await.map_err(failed)?;
        return Ok(cut(output.columns, output.rows, limits.rows));
    }

    // A cursor, so that the server stops after the rows wanted instead of producing them all
    let statement = split_statements(sql).into_iter().next().map(|s| s.text).unwrap_or_default();
    client
        .batch_execute(&format!("DECLARE {CURSOR} NO SCROLL CURSOR FOR {statement}\n"))
        .await
        .map_err(failed)?;
    let fetch = format!("FETCH {wanted} FROM {CURSOR}");
    let prepared = client.prepare(&fetch).await.map_err(failed)?;
    let columns = columns_from_statement(prepared.columns());
    let rows = if prepared.columns().iter().all(|c| is_native(c.type_())) {
        client.query(&prepared, &[]).await.map_err(failed)?.iter().map(row_to_json_values).collect()
    } else {
        let types: Vec<Type> = prepared.columns().iter().map(|c| c.type_().clone()).collect();
        let messages = client.simple_query(&fetch).await.map_err(failed)?;
        messages
            .iter()
            .filter_map(|message| match message {
                SimpleQueryMessage::Row(row) => Some(
                    types
                        .iter()
                        .enumerate()
                        .map(|(i, ty)| match row.try_get(i).ok().flatten() {
                            Some(text) => text_to_json(ty, text),
                            None => JsonValue::Null,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .collect()
    };
    Ok(cut(columns, rows, limits.rows))
}

/// At most `rows` of what was read, and whether there was more.
fn cut(columns: Vec<crate::db::model::QueryColumn>, mut rows: Vec<Vec<JsonValue>>, limit: usize) -> ReadResult {
    let more = rows.len() > limit;
    rows.truncate(limit);
    ReadResult { columns, rows, more }
}

#[cfg(test)]
mod server_tests {
    use super::super::session::Endpoint;
    use super::*;
    use crate::db::sql::reading::{READ_REFUSED, TIMED_OUT};
    use crate::db::Driver;

    fn endpoint() -> Endpoint {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL").expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        Endpoint { url: url::Url::parse(&url).unwrap(), ssh: None, known_hosts: None }
    }

    async fn side_client() -> Client {
        let (client, connection) = tokio_postgres::connect(endpoint().url.as_str(), tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    fn limits(rows: usize) -> ReadLimits {
        ReadLimits { rows, timeout: Duration::from_secs(10) }
    }

    /// A table of 1000 rows with a name no other test uses, dropped by `finish`.
    struct Fixture {
        side: Client,
        table: String,
        driver: Arc<PostgresDriver>,
    }

    impl Fixture {
        async fn new() -> Fixture {
            let side = side_client().await;
            let table = format!("dbunny_test_ai_{}", uuid::Uuid::new_v4().simple());
            side.batch_execute(&format!(
                "CREATE TABLE {table} (a int, b text); INSERT INTO {table} SELECT g, 'v' || g FROM generate_series(1, 1000) g"
            ))
            .await
            .unwrap();
            let driver = PostgresDriver::open(endpoint(), None).await.unwrap();
            Fixture { side, table, driver }
        }

        async fn count(&self) -> i64 {
            self.side.query_one(&format!("SELECT count(*) FROM {}", self.table), &[]).await.unwrap().get(0)
        }

        async fn finish(self) {
            self.side.batch_execute(&format!("DROP TABLE {}", self.table)).await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn reads_rows_and_columns() {
        let f = Fixture::new().await;
        let result = f.driver.reader().unwrap().query("SELECT g AS n, 'x'::text AS t FROM generate_series(1, 1000) g", limits(10)).await.unwrap();
        assert_eq!(result.rows.len(), 10);
        assert_eq!(result.columns[0].name, "n");
        assert!(result.more);
        let all = f.driver.reader().unwrap().query(&format!("SELECT a FROM {} WHERE a <= 3 ORDER BY a;", f.table), limits(10)).await.unwrap();
        assert_eq!((all.rows.len(), all.more), (3, false));
        // Types the binary decoders don't know come back as text
        let odd = f.driver.reader().unwrap().query("SELECT '1 day'::interval AS i, '10.0.0.1'::inet AS ip", limits(10)).await.unwrap();
        assert_eq!(odd.rows.len(), 1);
        let show = f.driver.reader().unwrap().query("SHOW server_version", limits(10)).await.unwrap();
        assert_eq!(show.rows.len(), 1);
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn stops_after_limit() {
        let f = Fixture::new().await;
        let started = std::time::Instant::now();
        let result = f.driver.reader().unwrap().query("SELECT g FROM generate_series(1, 50000000) g", limits(100)).await.unwrap();
        assert_eq!(result.rows.len(), 100);
        assert!(result.more);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn refuses_writes_before_running() {
        let f = Fixture::new().await;
        let err = f.driver.reader().unwrap().query(&format!("DELETE FROM {}", f.table), limits(10)).await.unwrap_err();
        assert_eq!(err, READ_REFUSED);
        assert_eq!(f.count().await, 1000);
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn writes_are_refused_by_the_transaction() {
        let f = Fixture::new().await;
        let seq = format!("dbunny_test_seq_{}", uuid::Uuid::new_v4().simple());
        f.side.batch_execute(&format!("CREATE SEQUENCE {seq}")).await.unwrap();
        let err = f.driver.read_unchecked(&format!("SELECT nextval('{seq}')"), limits(10)).await.unwrap_err();
        assert!(err.contains("read-only transaction"), "{err}");
        f.side.batch_execute(&format!("DROP SEQUENCE {seq}")).await.unwrap();
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn data_modifying_cte_is_refused() {
        let f = Fixture::new().await;
        let sql = format!("WITH d AS (DELETE FROM {t} RETURNING *) SELECT count(*) FROM d", t = f.table);
        assert!(f.driver.read_unchecked(&sql, limits(10)).await.is_err());
        assert_eq!(f.count().await, 1000);
        assert_eq!(f.driver.reader().unwrap().query(&sql, limits(10)).await.unwrap_err(), READ_REFUSED);
        assert_eq!(f.count().await, 1000);
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn times_out() {
        let f = Fixture::new().await;
        let short = ReadLimits { rows: 10, timeout: Duration::from_millis(500) };
        let err = f.driver.reader().unwrap().query("SELECT pg_sleep(5)", short).await.unwrap_err();
        assert_eq!(err, TIMED_OUT);
        let again = f.driver.reader().unwrap().query("SELECT 1", limits(10)).await.unwrap();
        assert_eq!(again.rows.len(), 1);
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn editor_transaction_is_untouched() {
        let f = Fixture::new().await;
        let (transactions, console) = (f.driver.transactions().unwrap(), f.driver.console().unwrap());
        transactions.begin().await.unwrap();
        console.execute(&format!("INSERT INTO {} VALUES (5000, 'new')", f.table)).await.unwrap();
        let count = f.driver.reader().unwrap().query(&format!("SELECT count(*) FROM {}", f.table), limits(10)).await.unwrap();
        assert_eq!(count.rows[0][0], serde_json::json!(1000));
        assert!(transactions.is_open().await.unwrap());
        transactions.rollback().await.unwrap();
        f.finish().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn reader_runs_after_its_connection_is_lost() {
        let f = Fixture::new().await;
        let reader = f.driver.reader().unwrap();
        let pid: i32 = {
            let first = reader.query("SELECT pg_backend_pid()", limits(1)).await.unwrap();
            first.rows[0][0].as_i64().unwrap() as i32
        };
        f.side.execute("SELECT pg_terminate_backend($1)", &[&pid]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let again = reader.query("SELECT 1", limits(1)).await.unwrap();
        assert_eq!(again.rows.len(), 1);
        f.finish().await;
    }
}
