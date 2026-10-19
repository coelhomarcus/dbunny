//! The SQL console on SQLite: running the editor's scripts, cancelling them and query plans.
//! Everything runs on the editor's connection, which also keeps its transaction.

use std::sync::atomic::Ordering;
use std::time::Instant;

use async_trait::async_trait;
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::Value as JsonValue;

use super::errors::{code_name, map_statement_error};
use super::split::split_sqlite;
use super::values::{standard_type, value_to_json};
use super::SqliteDriver;
use crate::db::model::{ExplainResult, QueryColumn, QueryResult};
use crate::db::{unsupported, Console, Engine, ExplainFormat};

/// What one statement gave back.
struct StatementOutput {
    columns: Vec<QueryColumn>,
    rows: Vec<Vec<JsonValue>>,
    /// The rows a change touched; None for a statement that returns rows.
    rows_affected: Option<u64>,
}

/// Runs one statement. One with result columns (a query, a PRAGMA, a change with its own
/// RETURNING) gives its rows; any other gives how many rows it changed.
fn run_statement(c: &Connection, sql: &str) -> rusqlite::Result<StatementOutput> {
    let mut stmt = c.prepare(sql)?;
    let width = stmt.column_count();
    if width == 0 {
        let before = c.total_changes();
        stmt.execute([])?;
        // `changes()` keeps the count of the last INSERT, UPDATE or DELETE, so a statement
        // that changed nothing (a CREATE, a BEGIN) would repeat it
        let affected = if c.total_changes() != before { c.changes() } else { 0 };
        return Ok(StatementOutput { columns: vec![], rows: vec![], rows_affected: Some(affected) });
    }

    let names: Vec<(String, Option<String>)> =
        stmt.columns().iter().map(|col| (col.name().to_string(), col.decl_type().map(str::to_string))).collect();
    // A column without a declared type (an expression) is named by its first value
    let mut seen: Vec<Option<&'static str>> = vec![None; width];
    let mut rows = Vec::new();
    let mut cursor = stmt.query([])?;
    while let Some(row) = cursor.next()? {
        let mut values = Vec::with_capacity(width);
        for (i, seen) in seen.iter_mut().enumerate() {
            let value = row.get_ref(i)?;
            if seen.is_none() {
                *seen = stored_type(value);
            }
            values.push(value_to_json(value));
        }
        rows.push(values);
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
    Ok(StatementOutput { columns, rows, rows_affected: None })
}

/// The type name the screens know for a value SQLite stored as integer, real, text or blob.
pub(super) fn stored_type(value: ValueRef<'_>) -> Option<&'static str> {
    match value {
        ValueRef::Null => None,
        ValueRef::Integer(_) => Some("bigint"),
        ValueRef::Real(_) => Some("double precision"),
        ValueRef::Text(_) => Some("text"),
        ValueRef::Blob(_) => Some("bytea"),
    }
}

/// An error of the editor's script as JSON for the UI, in the shape PostgreSQL's take.
/// `statement` is the (1-based index, total) of the failing statement when the script had
/// more than one. SQLite gives no position. A lock that stops the editor is never its own
/// transaction's, so it's another program's. `detail` adds to SQLite's message.
fn script_error(e: &rusqlite::Error, statement: Option<(usize, usize)>, detail: Option<&str>) -> String {
    let (failure, message) = match e {
        rusqlite::Error::SqliteFailure(failure, _) => (failure, map_statement_error(e, false)),
        // A syntax error: SQLite's message, without the statement and offset rusqlite adds
        rusqlite::Error::SqlInputError { error, msg, .. } => (error, msg.clone()),
        other => return map_statement_error(other, false),
    };
    serde_json::json!({
        "message": message,
        "severity": "ERROR",
        "detail": detail,
        "hint": null,
        "position": null,
        "code": code_name(failure.extended_code),
        "statement": statement.map(|(index, _)| index),
        "statementCount": statement.map(|(_, total)| total),
    })
    .to_string()
}

/// The error SQLite gives a statement it stopped.
fn interrupted() -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERRUPT), None)
}

fn is_interrupt(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(failure, _) if failure.code == rusqlite::ErrorCode::OperationInterrupted)
}

#[async_trait]
impl Console for SqliteDriver {
    /// Runs the script's statements in order on the editor's connection. The rows of the
    /// last statement that returns rows are shown; the others only add to the affected
    /// count. A change shows its rows only when it says RETURNING itself.
    async fn execute(&self, sql: &str) -> Result<QueryResult, String> {
        let statements = split_sqlite(sql);
        if statements.is_empty() {
            return Err("SQL query is required".into());
        }
        let state = self.editor().transaction_state();
        let cancelled = self.editor().start_script();
        self.editor()
            .run(move |c| {
                let total = statements.len();
                let start = Instant::now();
                let mut shown: Option<StatementOutput> = None;
                let mut affected: u64 = 0;
                for (index, statement) in statements.iter().enumerate() {
                    let position = (total > 1).then_some((index + 1, total));
                    // A cancel that came between two statements, which SQLite doesn't keep
                    if cancelled.load(Ordering::SeqCst) {
                        return Err(script_error(&interrupted(), position, None));
                    }
                    let in_transaction = !c.is_autocommit();
                    let output = run_statement(c, &statement.text);
                    // A table save that finds the file busy meanwhile names the right holder
                    state.note(c);
                    let output = output.map_err(|e| {
                        // SQLite rolls the whole transaction back when it stops a change
                        let rolled_back = in_transaction && c.is_autocommit() && is_interrupt(&e);
                        script_error(&e, position, rolled_back.then_some("The transaction was rolled back."))
                    })?;
                    match output.rows_affected {
                        Some(count) => affected += count,
                        None => shown = Some(output),
                    }
                }
                let duration = start.elapsed().as_millis() as u64;
                let in_transaction = !c.is_autocommit();

                Ok(match shown {
                    Some(output) => QueryResult {
                        row_count: output.rows.len(),
                        columns: output.columns,
                        rows: output.rows,
                        rows_affected: output.rows_affected,
                        duration,
                        statement_count: total,
                        notices: vec![],
                        in_transaction,
                    },
                    None => QueryResult {
                        columns: vec![],
                        rows: vec![],
                        row_count: affected as usize,
                        rows_affected: Some(affected),
                        duration,
                        statement_count: total,
                        notices: vec![],
                        in_transaction,
                    },
                })
            })
            .await
    }

    /// Stops whatever the editor's connection is running, and the rest of its script; it
    /// fails with "canceling statement due to user request". With nothing running it does
    /// nothing: the next script starts afresh.
    async fn cancel(&self) -> Result<(), String> {
        self.editor().cancel();
        Ok(())
    }

    /// `EXPLAIN QUERY PLAN` for one statement, which doesn't run it.
    async fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainResult, String> {
        if analyze {
            return Err(unsupported(Engine::Sqlite, "EXPLAIN ANALYZE"));
        }
        let statements = split_sqlite(sql);
        let statement = match statements.as_slice() {
            [] => return Err("SQL query is required".into()),
            [one] => one,
            _ => return Err("EXPLAIN works on one statement at a time. Select the one to explain.".into()),
        };
        if statement.text.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("explain")) {
            return Err("Leave out EXPLAIN: the plan is requested for you.".into());
        }

        let explain_sql = format!("EXPLAIN QUERY PLAN {}", statement.text);
        let plan = self
            .editor()
            .run(move |c| {
                let read = || -> rusqlite::Result<Vec<JsonValue>> {
                    let mut stmt = c.prepare(&explain_sql)?;
                    let rows = stmt.query_map([], |r| {
                        Ok(serde_json::json!({
                            "id": r.get::<_, i64>("id")?,
                            "parent": r.get::<_, i64>("parent")?,
                            "detail": r.get::<_, String>("detail")?,
                        }))
                    })?;
                    rows.collect()
                };
                read().map_err(|e| script_error(&e, None, None))
            })
            .await?;
        Ok(ExplainResult { plan: JsonValue::Array(plan), analyzed: false, format: ExplainFormat::SqliteQueryPlan })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use serde_json::{json, Value as JsonValue};

    use super::super::tests_support::{stop_until_done, without_hanging, TempDb};
    use super::super::OpenOptions;
    use super::*;
    use crate::db::{Driver, ExplainFormat};

    async fn open(f: &TempDb) -> Arc<SqliteDriver> {
        SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap()
    }

    fn error(raw: &str) -> JsonValue {
        serde_json::from_str(raw).unwrap_or_else(|_| panic!("not a JSON error: {raw}"))
    }

    fn types(result: &QueryResult) -> Vec<&str> {
        result.columns.iter().map(|c| c.data_type.as_str()).collect()
    }

    const ENDLESS: &str = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) SELECT count(*) FROM n";

    #[tokio::test]
    async fn scripts_run_in_order_and_report_changes() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, score REAL)");
        let d = open(&f).await;
        let console = d.console().unwrap();

        // Only changes: their counts add up, and a CREATE adds nothing
        let r = console.execute("INSERT INTO t (name) VALUES ('a'), ('b'); UPDATE t SET score = 1.5; CREATE TABLE u (x)").await.unwrap();
        assert_eq!((r.rows_affected, r.row_count, r.statement_count), (Some(4), 4, 3));
        assert!(r.columns.is_empty() && r.rows.is_empty() && r.notices.is_empty());
        assert!(!r.in_transaction);

        // A RETURNING of its own brings its rows; the last statement with rows is shown
        let r = console
            .execute("INSERT INTO t (name) VALUES ('c') RETURNING id, name, score, 1 + 1 AS two; UPDATE t SET name = upper(name)")
            .await
            .unwrap();
        assert_eq!(r.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "name", "score", "two"]);
        assert_eq!(r.rows, vec![vec![json!(3), json!("c"), JsonValue::Null, json!(2)]]);
        assert_eq!((r.rows_affected, r.row_count, r.statement_count), (None, 1, 2));

        let r = console.execute("UPDATE t SET score = 2 WHERE id = 1; SELECT id, name, score FROM t ORDER BY id").await.unwrap();
        assert_eq!(r.rows, vec![vec![json!(1), json!("A"), json!(2.0)], vec![json!(2), json!("B"), json!(1.5)], vec![json!(3), json!("C"), JsonValue::Null]]);
        assert_eq!(types(&r), ["bigint", "text", "double precision"]);
        assert_eq!(r.rows_affected, None);

        // A plain change shows no rows, only how many it changed
        let r = console.execute("UPDATE t SET score = 3 WHERE id > 1").await.unwrap();
        assert_eq!((r.rows_affected, r.rows.len(), r.statement_count), (Some(2), 0, 1));
        let r = console.execute("DELETE FROM t WHERE id = -1").await.unwrap();
        assert_eq!(r.rows_affected, Some(0));

        // Expressions have no declared type: the values tell it
        let r = console.execute("SELECT 1, 1.5, 'x', x'00ff', NULL, 9007199254740993").await.unwrap();
        assert_eq!(types(&r), ["bigint", "double precision", "text", "bytea", "text", "bigint"]);
        assert_eq!(r.rows[0], vec![json!(1), json!(1.5), json!("x"), json!("\\x00ff"), JsonValue::Null, json!("9007199254740993")]);

        assert_eq!(console.execute("  -- nothing\n ; ").await.err().unwrap(), "SQL query is required");
    }

    #[tokio::test]
    async fn errors_name_the_failing_statement() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL)");
        let d = open(&f).await;
        let console = d.console().unwrap();

        let err = console
            .execute("INSERT INTO t (name) VALUES ('a'); INSERT INTO t (name) VALUES (NULL); SELECT 1")
            .await
            .err().unwrap();
        assert_eq!(
            error(&err),
            json!({
                "message": "NOT NULL constraint failed: t.name",
                "severity": "ERROR",
                "detail": null,
                "hint": null,
                "position": null,
                "code": "SQLITE_CONSTRAINT_NOTNULL",
                "statement": 2,
                "statementCount": 3,
            })
        );
        // The statements before it ran
        assert_eq!(d.data().count_rows("main", "t").await.unwrap(), 1);

        // One statement: no statement number, and SQLite's own text
        let e = error(&console.execute("SELEC 1").await.err().unwrap());
        assert_eq!((e["message"].as_str(), e["code"].as_str()), (Some("near \"SELEC\": syntax error"), Some("SQLITE_ERROR")));
        assert_eq!((e["statement"].clone(), e["statementCount"].clone()), (JsonValue::Null, JsonValue::Null));
        let e = error(&console.execute("SELECT 1; SELECT * FROM nope").await.err().unwrap());
        assert_eq!((e["message"].as_str(), e["statement"].as_u64()), (Some("no such table: nope"), Some(2)));

        // The editor is still usable
        assert_eq!(console.execute("SELECT count(*) FROM t").await.unwrap().rows, vec![vec![json!(1)]]);
    }

    #[test]
    fn cancelling_stops_a_running_query() {
        without_hanging(async {
            let f = TempDb::with("CREATE TABLE t (x)");
            let d = open(&f).await;
            let mut running = {
                let d = d.clone();
                tokio::spawn(async move { d.console().unwrap().execute(&format!("SELECT 1; {ENDLESS}")).await })
            };
            tokio::time::sleep(Duration::from_millis(200)).await;
            let started = Instant::now();
            // Until it stops: a cancel that comes before the script starts is forgotten
            let (e, _) = cancel_until_stopped(&d, &mut running, SOON).await;
            assert_eq!(e["message"], "canceling statement due to user request");
            assert_eq!(e["code"].as_str(), Some("SQLITE_INTERRUPT"));
            // Normally it is the endless second statement; on a slow machine the cancel can land
            // before the script's first one has started, which stops it there
            assert!(matches!(e["statement"].as_u64(), Some(1 | 2)), "{e}");
            assert!(started.elapsed() < Duration::from_secs(2));

            // Cancelling with nothing running changes nothing, and the next query runs
            d.console().unwrap().cancel().await.unwrap();
            assert_eq!(d.console().unwrap().execute("SELECT 2").await.unwrap().rows, vec![vec![json!(2)]]);
        });
    }

    /// Runs `sql` on the editor in a task of its own.
    fn start(d: &Arc<SqliteDriver>, sql: String) -> tokio::task::JoinHandle<Result<QueryResult, String>> {
        let d = d.clone();
        tokio::spawn(async move { d.console().unwrap().execute(&sql).await })
    }

    /// Cancels like the editor's button does, each `every` until `running` ends; gives its
    /// error and how many cancels it took.
    async fn cancel_until_stopped(
        d: &Arc<SqliteDriver>,
        running: &mut tokio::task::JoinHandle<Result<QueryResult, String>>,
        every: Duration,
    ) -> (JsonValue, usize) {
        let cancel = || {
            let d = d.clone();
            async move { d.console().unwrap().cancel().await.unwrap() }
        };
        let (outcome, cancels) = stop_until_done(running, every, Duration::from_secs(5), cancel).await;
        (error(&outcome.err().expect("the script was stopped")), cancels)
    }

    const SOON: Duration = Duration::from_millis(20);

    #[test]
    fn cancelling_stops_a_script_between_statements() {
        without_hanging(async {
            let f = TempDb::with("CREATE TABLE t (x)");
            let d = open(&f).await;
            // Each statement is short, so a cancel mostly lands after its last check for
            // one, or between two statements, where SQLite forgets it
            let total = 200_000;
            let mut running = start(&d, "INSERT INTO t VALUES (1);\n".repeat(total));
            tokio::time::sleep(Duration::from_millis(100)).await;
            let started = Instant::now();
            let (e, cancels) = cancel_until_stopped(&d, &mut running, Duration::from_millis(250)).await;
            assert_eq!(cancels, 1, "the first cancel stops the script");
            assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());

            assert_eq!(e["message"], "canceling statement due to user request");
            assert_eq!((e["code"].as_str(), e["statementCount"].as_u64()), (Some("SQLITE_INTERRUPT"), Some(total as u64)));
            assert_eq!(e["detail"], JsonValue::Null);
            let inserted = d.data().count_rows("main", "t").await.unwrap() as u64;
            assert!(inserted < total as u64 / 2, "{inserted} of {total} rows were inserted");
            assert_eq!(e["statement"].as_u64(), Some(inserted + 1), "the statements before the one named ran");
        });
    }

    #[test]
    fn a_cancel_with_nothing_running_does_not_stop_the_next_script() {
        without_hanging(async {
            let f = TempDb::with("CREATE TABLE t (x)");
            let d = open(&f).await;
            let console = d.console().unwrap();
            console.cancel().await.unwrap();
            let r = console.execute("INSERT INTO t VALUES (1); INSERT INTO t VALUES (2); SELECT count(*) FROM t").await.unwrap();
            assert_eq!(r.rows, vec![vec![json!(2)]]);

            // Nor does one that came after a script it stopped had ended
            let mut running = start(&d, format!("SELECT 1; {ENDLESS}"));
            cancel_until_stopped(&d, &mut running, SOON).await;
            console.cancel().await.unwrap();
            let r = console.execute("INSERT INTO t VALUES (3); SELECT count(*) FROM t").await.unwrap();
            assert_eq!(r.rows, vec![vec![json!(3)]]);
        });
    }

    #[test]
    fn a_cancelled_write_says_it_rolled_the_transaction_back() {
        without_hanging(async {
            let f = TempDb::with("CREATE TABLE t (x); INSERT INTO t VALUES (1);");
            let d = open(&f).await;
            let console = d.console().unwrap();
            assert!(console.execute("BEGIN; INSERT INTO t VALUES (2)").await.unwrap().in_transaction);

            // A cancelled read leaves the transaction as it was
            let mut running = start(&d, ENDLESS.to_string());
            let (e, _) = cancel_until_stopped(&d, &mut running, SOON).await;
            assert_eq!(e["message"], "canceling statement due to user request");
            assert_eq!(e["detail"], JsonValue::Null);
            assert!(d.editor().in_transaction());
            assert!(d.transactions().unwrap().is_open().await.unwrap());

            // A cancelled write takes the whole transaction with it, and says so
            let mut running = start(&d, format!("UPDATE t SET x = ({ENDLESS})"));
            let (e, _) = cancel_until_stopped(&d, &mut running, SOON).await;
            assert_eq!(e["message"], "canceling statement due to user request");
            assert_eq!(e["detail"], "The transaction was rolled back.");
            assert_eq!((e["code"].as_str(), e["statement"].clone()), (Some("SQLITE_INTERRUPT"), JsonValue::Null));
            assert!(!d.editor().in_transaction());
            assert!(!d.transactions().unwrap().is_open().await.unwrap());
            let r = console.execute("SELECT x FROM t").await.unwrap();
            assert_eq!((r.rows, r.in_transaction), (vec![vec![json!(1)]], false), "the insert before it was undone too");
        });
    }

    #[tokio::test]
    async fn explain_query_plan_is_a_tree() {
        let f = TempDb::with(
            "CREATE TABLE a (id INTEGER PRIMARY KEY, x); CREATE TABLE b (id INTEGER PRIMARY KEY, a_id); CREATE INDEX b_a ON b (a_id);",
        );
        let d = open(&f).await;
        let console = d.console().unwrap();

        let sql = "SELECT * FROM a WHERE id IN (SELECT a_id FROM b WHERE a_id > 1) ORDER BY x";
        let explained = console.explain(&format!("  {sql};  "), false).await.unwrap();
        assert_eq!(explained.format, ExplainFormat::SqliteQueryPlan);
        assert!(!explained.analyzed);
        let rows = explained.plan.as_array().unwrap();
        assert!(rows.len() >= 2, "{rows:?}");
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        for row in rows {
            assert_eq!(row.as_object().unwrap().len(), 3, "{row}");
            assert!(row["detail"].is_string(), "{row}");
            let parent = row["parent"].as_i64().unwrap();
            assert!(parent == 0 || ids.contains(&parent), "the parent {parent} is another row");
        }
        assert!(rows.iter().any(|r| r["parent"].as_i64() != Some(0)), "a nested step: {rows:?}");
        assert!(rows.iter().any(|r| r["detail"].as_str().unwrap().contains("b_a")), "{rows:?}");
        // Nothing ran
        assert_eq!(console.explain("DELETE FROM a", false).await.unwrap().format, ExplainFormat::SqliteQueryPlan);

        assert_eq!(console.explain(sql, true).await.err().unwrap(), "EXPLAIN ANALYZE isn't supported by SQLite");
        assert_eq!(console.explain("", false).await.err().unwrap(), "SQL query is required");
        assert_eq!(
            console.explain("SELECT 1; SELECT 2", false).await.err().unwrap(),
            "EXPLAIN works on one statement at a time. Select the one to explain."
        );
        assert_eq!(
            console.explain("explain query plan SELECT 1", false).await.err().unwrap(),
            "Leave out EXPLAIN: the plan is requested for you."
        );
        let e = error(&console.explain("SELECT * FROM nope", false).await.err().unwrap());
        assert_eq!((e["message"].as_str(), e["statement"].clone()), (Some("no such table: nope"), JsonValue::Null));
    }
}
