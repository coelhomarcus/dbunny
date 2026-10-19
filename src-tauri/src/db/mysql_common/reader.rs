//! Reads for the assistant: one statement that only reads, in a read-only transaction that is
//! always rolled back, on a connection from the pool (never the editor's).

use std::time::Duration;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Column, Conn, Error as MysqlError};

use super::driver::ServerDriver;
use super::errors::error_message;
use super::values::{type_name, value_to_json};
use crate::db::model::{ExplainResult, QueryColumn, ReadLimits, ReadResult};
use crate::db::sql::reading::{check_reading, TIMED_OUT};
use crate::db::sql::Dialect;
use crate::db::ReadOnlyQueries;

/// How long past the statement's own limit the client waits before killing it.
const CLIENT_GRACE: Duration = Duration::from_secs(2);
/// MySQL's "maximum statement execution time exceeded" and MariaDB's "max_statement_time exceeded".
const TIMEOUT_CODES: [u16; 2] = [3024, 1969];

/// How a read ended on its connection.
enum Finished {
    /// All the rows were read; the transaction was rolled back and the limit lifted.
    Whole(ReadResult),
    /// Stopped at the row limit with the statement still producing rows.
    Cut(ReadResult),
}

impl ServerDriver {
    /// Runs `sql` without asking the classifier first. The read-only transaction refuses to
    /// write, which the tests rely on.
    pub(crate) async fn read_unchecked(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        let mut conn = self.conn().await?;
        let id = conn.id();
        let flavor = self.flavor();
        let ms = limits.timeout.as_millis() as u64;
        let (limit_sql, lift_sql) = (flavor.statement_timeout_sql(ms), flavor.statement_timeout_sql(0));
        let ran = tokio::time::timeout(limits.timeout + CLIENT_GRACE, read_on(&mut conn, &limit_sql, &lift_sql, sql, limits.rows)).await;
        match ran {
            Ok(Ok(Finished::Whole(result))) => Ok(result),
            Ok(Ok(Finished::Cut(result))) => {
                // The server is still producing rows nobody reads: stop it, and let the connection go
                self.kill_query(id).await;
                let _ = conn.disconnect().await;
                Ok(result)
            }
            Ok(Err(e)) => {
                let _ = conn.disconnect().await;
                Err(self.read_failure(&e))
            }
            Err(_) => {
                self.kill_query(id).await;
                let _ = conn.disconnect().await;
                Err(TIMED_OUT.to_string())
            }
        }
    }

    /// EXPLAIN ANALYZE of `sql`, which runs it, under the terms of `read_unchecked`.
    pub(crate) async fn explain_unchecked(&self, sql: &str, limits: ReadLimits) -> Result<ExplainResult, String> {
        let mut conn = self.conn().await?;
        let id = conn.id();
        let flavor = self.flavor();
        let ms = limits.timeout.as_millis() as u64;
        let (limit_sql, lift_sql) = (flavor.statement_timeout_sql(ms), flavor.statement_timeout_sql(0));
        let explain_sql = flavor.explain_sql(sql, true);
        let work = async {
            conn.query_drop(limit_sql).await?;
            conn.query_drop("START TRANSACTION READ ONLY").await?;
            let plan = conn.query_first::<String, _>(explain_sql).await?;
            conn.query_drop("ROLLBACK").await?;
            conn.query_drop(lift_sql).await?;
            Ok::<_, MysqlError>(plan)
        };
        match tokio::time::timeout(limits.timeout + CLIENT_GRACE, work).await {
            Ok(Ok(plan)) => flavor.plan_of(&plan.ok_or("The server returned no plan")?, true),
            Ok(Err(e)) => {
                let message = self.read_failure(&e);
                let _ = conn.disconnect().await;
                Err(message)
            }
            Err(_) => {
                self.kill_query(id).await;
                let _ = conn.disconnect().await;
                Err(TIMED_OUT.to_string())
            }
        }
    }

    /// What a failed read says.
    fn read_failure(&self, e: &MysqlError) -> String {
        match e {
            MysqlError::Server(server) if TIMEOUT_CODES.contains(&server.code) => TIMED_OUT.to_string(),
            _ => error_message(e, self.endpoint()),
        }
    }

    /// Stops what connection `id` is running, from another connection of the pool.
    async fn kill_query(&self, id: u32) {
        if let Ok(mut other) = self.conn().await {
            let _ = other.query_drop(format!("KILL QUERY {id}")).await;
        }
    }
}

#[async_trait]
impl ReadOnlyQueries for ServerDriver {
    async fn query(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String> {
        check_reading(Dialect::Mysql, sql).map_err(|refusal| refusal.message().to_string())?;
        self.read_unchecked(sql, limits).await
    }

    async fn explain_analyzed(&self, sql: &str, limits: ReadLimits) -> Result<ExplainResult, String> {
        check_reading(Dialect::Mysql, sql).map_err(|refusal| refusal.message().to_string())?;
        self.explain_unchecked(sql, limits).await
    }
}

/// The statement limit, START TRANSACTION READ ONLY, the statement read up to `rows + 1` rows,
/// and ROLLBACK.
async fn read_on(conn: &mut Conn, limit_sql: &str, lift_sql: &str, sql: &str, rows: usize) -> Result<Finished, MysqlError> {
    conn.query_drop(limit_sql).await?;
    conn.query_drop("START TRANSACTION READ ONLY").await?;
    let mut out = Vec::new();
    let mut more = false;
    let info: Vec<QueryColumn>;
    {
        let mut result = conn.query_iter(sql).await?;
        let columns: Vec<Column> = result.columns_ref().to_vec();
        info = columns.iter().map(|c| QueryColumn { name: c.name_str().to_string(), data_type: type_name(c) }).collect();
        while let Some(row) = result.next().await? {
            if out.len() == rows {
                more = true;
                break;
            }
            out.push(
                row.unwrap()
                    .iter()
                    .zip(columns.iter())
                    .zip(info.iter())
                    .map(|((value, column), described)| described.capped(value_to_json(value, column)))
                    .collect(),
            );
        }
    }
    let result = ReadResult { columns: info, rows: out, more };
    if more {
        return Ok(Finished::Cut(result));
    }
    conn.query_drop("ROLLBACK").await?;
    conn.query_drop(lift_sql).await?;
    Ok(Finished::Whole(result))
}

#[cfg(test)]
mod tests {
    use crate::db::mariadb::MariadbFlavor;
    use crate::db::mysql::MysqlFlavor;
    use crate::db::mysql_common::flavor::Flavor;

    #[test]
    fn each_flavor_limits_statement_time_in_its_own_unit() {
        assert_eq!(MysqlFlavor.statement_timeout_sql(30_000), "SET SESSION max_execution_time = 30000");
        assert_eq!(MysqlFlavor.statement_timeout_sql(0), "SET SESSION max_execution_time = 0");
        assert_eq!(MariadbFlavor.statement_timeout_sql(30_000), "SET SESSION max_statement_time = 30.000");
        assert_eq!(MariadbFlavor.statement_timeout_sql(1_500), "SET SESSION max_statement_time = 1.500");
    }
}

#[cfg(test)]
mod server_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::db::mysql_common::tests_support::{exec, open_server, query_one, TestDb};
    use crate::db::sql::reading::{READ_REFUSED, TIMED_OUT};
    use crate::db::Driver;

    fn limits(rows: usize) -> ReadLimits {
        ReadLimits { rows, timeout: Duration::from_secs(10) }
    }

    /// A database with `t(a, b)` holding 1000 rows.
    async fn thousand() -> (TestDb, Arc<ServerDriver>) {
        let db = TestDb::new().await;
        exec(
            &db.endpoint,
            "CREATE TABLE t (a INT, b TEXT); \
             INSERT INTO t SELECT x, CONCAT('v', x) FROM \
             (WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 1000) SELECT x FROM n) q",
        )
        .await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        (db, driver)
    }

    async fn count(db: &TestDb) -> i64 {
        query_one::<i64>(&db.endpoint, "SELECT count(*) FROM t").await.unwrap()
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn reads_rows_and_columns() {
        let (db, d) = thousand().await;
        let result = d.reader().unwrap().query("SELECT a, b FROM t ORDER BY a", limits(10)).await.unwrap();
        assert_eq!(result.rows.len(), 10);
        assert_eq!(result.columns[0].name, "a");
        assert!(result.more);
        let all = d.reader().unwrap().query("SELECT a FROM t WHERE a <= 3 ORDER BY a;", limits(10)).await.unwrap();
        assert_eq!((all.rows.len(), all.more), (3, false));
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn stops_after_limit() {
        let (db, d) = thousand().await;
        let started = Instant::now();
        let result = d.reader().unwrap().query("SELECT x.a FROM t x, t y, t z", limits(100)).await.unwrap();
        assert_eq!(result.rows.len(), 100);
        assert!(result.more);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        // The session still works afterwards
        assert_eq!(d.reader().unwrap().query("SELECT 1", limits(1)).await.unwrap().rows.len(), 1);
        assert_eq!(d.data().count_rows(&db.name, "t").await, Ok(1000));
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn writes_are_refused_by_the_transaction() {
        let (db, d) = thousand().await;
        let err = d.read_unchecked("DELETE FROM t", limits(10)).await.unwrap_err();
        assert!(err.to_lowercase().contains("read-only"), "{err}");
        assert_eq!(count(&db).await, 1000);
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn explain_analyze_runs_in_the_read_only_transaction() {
        let (db, d) = thousand().await;
        let reader = d.reader().unwrap();
        // A write is never analyzed, whether the server takes it or not
        let _ = d.explain_unchecked("DELETE FROM t", limits(10)).await;
        assert_eq!(count(&db).await, 1000);
        let plan = reader.explain_analyzed("SELECT count(*) FROM t", limits(10)).await.unwrap();
        assert!(plan.analyzed);
        assert_eq!(reader.explain_analyzed("DELETE FROM t", limits(10)).await.err().unwrap(), READ_REFUSED);
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn executable_comments_do_not_hide_a_write_of_a_file() {
        let (db, d) = thousand().await;
        let sql = "SELECT a FROM t /*M! INTO OUTFILE '/tmp/dbunny_never.txt' */";
        assert_eq!(d.reader().unwrap().query(sql, limits(10)).await.unwrap_err(), READ_REFUSED);
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn ddl_never_reaches_the_server() {
        let (db, d) = thousand().await;
        let err = d.reader().unwrap().query("CREATE TABLE dbunny_ai_ddl (a INT)", limits(10)).await.unwrap_err();
        assert_eq!(err, READ_REFUSED);
        let exists = query_one::<String>(
            &db.endpoint,
            "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'dbunny_ai_ddl'",
        )
        .await;
        assert_eq!(exists, None);
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn times_out() {
        let (db, d) = thousand().await;
        let short = ReadLimits { rows: 10, timeout: Duration::from_millis(500) };
        let err = d.reader().unwrap().query("SELECT SUM(w.a * x.a + y.a * z.a) FROM t w, t x, t y, t z", short).await.unwrap_err();
        assert_eq!(err, TIMED_OUT);
        assert_eq!(d.reader().unwrap().query("SELECT 1", limits(1)).await.unwrap().rows.len(), 1);
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn editor_transaction_is_untouched() {
        let (db, d) = thousand().await;
        let (transactions, console) = (d.transactions().unwrap(), d.console().unwrap());
        transactions.begin().await.unwrap();
        console.execute("INSERT INTO t VALUES (5000, 'new')").await.unwrap();
        let seen = d.reader().unwrap().query("SELECT count(*) FROM t", limits(1)).await.unwrap();
        assert_eq!(seen.rows[0][0], serde_json::json!(1000));
        assert!(transactions.is_open().await.unwrap());
        transactions.rollback().await.unwrap();
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn show_and_describe_read() {
        let (db, d) = thousand().await;
        let tables = d.reader().unwrap().query("SHOW TABLES", limits(10)).await.unwrap();
        assert_eq!(tables.rows.len(), 1);
        let described = d.reader().unwrap().query("DESCRIBE t", limits(10)).await.unwrap();
        assert_eq!(described.rows.len(), 2);
        db.drop_now().await;
    }
}
