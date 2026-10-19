//! Reads for the assistant: one statement that only reads, on a connection that can't write.

use std::time::Duration;

use async_trait::async_trait;
use rusqlite::Connection;

use super::conn::SqliteConn;
use super::console::stored_type;
use super::values::{standard_type, value_to_json};
use super::{OpenOptions, SqliteDriver};
use crate::db::model::{QueryColumn, ReadLimits, ReadResult};
use crate::db::sql::reading::{check_reading, TIMED_OUT};
use crate::db::sql::Dialect;
use crate::db::ReadOnlyQueries;

/// How often a statement past its timeout is interrupted again: SQLite forgets an interrupt
/// that comes before the statement starts.
const INTERRUPT_EVERY: Duration = Duration::from_millis(50);

impl SqliteDriver {
    /// Runs `sql` on the assistant's connection without asking the classifier first. The
    /// connection itself refuses to write, which the tests rely on.
    pub(super) async fn read_unchecked(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        let conn = self.reader_connection().await?;
        let sql = sql.to_string();
        let rows = limits.rows;
        let task = conn.run(move |c| read_rows(c, &sql, rows).map_err(|e| e.to_string()));
        tokio::pin!(task);
        if let Ok(result) = tokio::time::timeout(limits.timeout, &mut task).await {
            return result;
        }
        // Past the timeout: stop the statement, and keep stopping until the task ends
        loop {
            conn.interrupt();
            if tokio::time::timeout(INTERRUPT_EVERY, &mut task).await.is_ok() {
                return Err(TIMED_OUT.to_string());
            }
        }
    }

    /// The connection that can't write, opened the first time it is needed.
    async fn reader_connection(&self) -> Result<&SqliteConn, String> {
        self.reader
            .get_or_try_init(|| async {
                let path = self.path.clone();
                tokio::task::spawn_blocking(move || {
                    SqliteConn::open(&path, &OpenOptions { read_only: true, ..Default::default() })
                })
                .await
                .map_err(|e| format!("Opening the file stopped unexpectedly: {e}"))?
            })
            .await
    }
}

#[async_trait]
impl ReadOnlyQueries for SqliteDriver {
    async fn query(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        check_reading(Dialect::Sqlite, sql).map_err(|refusal| refusal.message().to_string())?;
        self.read_unchecked(sql, limits).await
    }
}

/// Steps through the statement until `rows + 1` rows, the extra one only to tell there were more.
fn read_rows(c: &Connection, sql: &str, rows: usize) -> rusqlite::Result<ReadResult> {
    let mut stmt = c.prepare(sql)?;
    let width = stmt.column_count();
    let names: Vec<(String, Option<String>)> =
        stmt.columns().iter().map(|col| (col.name().to_string(), col.decl_type().map(str::to_string))).collect();
    let mut seen: Vec<Option<&'static str>> = vec![None; width];
    let mut out = Vec::new();
    let mut more = false;
    let mut cursor = stmt.query([])?;
    while let Some(row) = cursor.next()? {
        if out.len() == rows {
            more = true;
            break;
        }
        let mut values = Vec::with_capacity(width);
        for (i, seen) in seen.iter_mut().enumerate() {
            let value = row.get_ref(i)?;
            if seen.is_none() {
                *seen = stored_type(value);
            }
            values.push(value_to_json(value));
        }
        out.push(values);
    }
    let columns = names
        .into_iter()
        .zip(seen)
        .map(|((name, declared), seen)| QueryColumn {
            name,
            data_type: match declared {
                Some(declared) => standard_type(&declared),
                None => seen.unwrap_or("text"),
            }
            .to_string(),
        })
        .collect();
    Ok(ReadResult { columns, rows: out, more })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::tests_support::TempDb;
    use super::super::OpenOptions;
    use super::*;
    use crate::db::sql::reading::READ_REFUSED;
    use crate::db::{Capabilities, Driver};

    const THOUSAND: &str = "CREATE TABLE t (a INTEGER, b TEXT); \
        WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 1000) \
        INSERT INTO t SELECT x, 'v' || x FROM n;";
    const ENDLESS: &str = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) SELECT count(*) FROM n";

    fn limits(rows: usize) -> ReadLimits {
        ReadLimits { rows, timeout: Duration::from_secs(10) }
    }

    async fn open(f: &TempDb) -> std::sync::Arc<SqliteDriver> {
        SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap()
    }

    #[tokio::test]
    async fn reads_rows_and_columns() {
        let f = TempDb::with(THOUSAND);
        let d = open(&f).await;
        let result = d.reader().unwrap().query("SELECT a, b FROM t ORDER BY a", limits(10)).await.unwrap();
        assert_eq!(result.rows.len(), 10);
        assert_eq!(result.columns[0].name, "a");
        assert_eq!(result.columns[1].name, "b");
        assert!(result.more);
        let all = d.reader().unwrap().query("SELECT a FROM t WHERE a <= 3", limits(10)).await.unwrap();
        assert_eq!((all.rows.len(), all.more), (3, false));
    }

    #[tokio::test]
    async fn stops_after_limit() {
        let f = TempDb::with("CREATE TABLE t (a)");
        let d = open(&f).await;
        let started = Instant::now();
        let sql = "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n) SELECT x FROM n";
        let result = d.reader().unwrap().query(sql, limits(100)).await.unwrap();
        assert_eq!(result.rows.len(), 100);
        assert!(result.more);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn refuses_writes_before_running() {
        let f = TempDb::with(THOUSAND);
        let d = open(&f).await;
        let err = d.reader().unwrap().query("DELETE FROM t", limits(10)).await.unwrap_err();
        assert_eq!(err, READ_REFUSED);
        let count = d.reader().unwrap().query("SELECT count(*) FROM t", limits(10)).await.unwrap();
        assert_eq!(count.rows[0][0], serde_json::json!(1000));
    }

    #[tokio::test]
    async fn reader_connection_cannot_write() {
        let f = TempDb::with(THOUSAND);
        let d = open(&f).await;
        let err = d.read_unchecked("INSERT INTO t VALUES (1, 'x')", limits(10)).await.unwrap_err();
        assert!(err.to_lowercase().contains("readonly"), "{err}");
    }

    #[tokio::test]
    async fn times_out() {
        let f = TempDb::with("CREATE TABLE t (a)");
        let d = open(&f).await;
        let short = ReadLimits { rows: 10, timeout: Duration::from_millis(200) };
        let err = d.reader().unwrap().query(ENDLESS, short).await.unwrap_err();
        assert_eq!(err, "The query was stopped after 30 s.");
        let again = d.reader().unwrap().query("SELECT 1", limits(10)).await.unwrap();
        assert_eq!(again.rows.len(), 1);
    }

    #[tokio::test]
    async fn editor_transaction_is_untouched() {
        let f = TempDb::with(THOUSAND);
        let d = open(&f).await;
        d.transactions().unwrap().begin().await.unwrap();
        d.console().unwrap().execute("INSERT INTO t VALUES (5000, 'new')").await.unwrap();
        let count = d.reader().unwrap().query("SELECT count(*) FROM t", limits(10)).await.unwrap();
        assert_eq!(count.rows[0][0], serde_json::json!(1000));
        assert!(d.transactions().unwrap().is_open().await.unwrap());
        d.transactions().unwrap().rollback().await.unwrap();
    }

    #[tokio::test]
    async fn the_session_has_the_assistant() {
        let f = TempDb::with("CREATE TABLE t (a)");
        let d = open(&f).await;
        assert!(Capabilities::sqlite().assistant && d.capabilities().assistant);
        assert!(d.reader().is_some());
    }
}
