//! The SQL editor of a MySQL session: scripts run a statement at a time on the editor's own
//! connection, a cancel kills the running statement from another one, and EXPLAIN reads the
//! server's plan as a tree.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Column, Conn, Error as MysqlError, Row};
use serde_json::Value as JsonValue;

use crate::db::mysql_common::editor::in_transaction_status;
use super::plan::is_reading_statement;
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::model::{ExplainResult, Notice, QueryColumn, QueryResult};
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;
use crate::db::mysql_common::split::{next_token, split_mysql, Kind};
use crate::db::mysql_common::values::{type_name, value_to_json};
use crate::db::sql::split::Statement;
use crate::db::sqlite::errors::CANCELLED;
use crate::db::{unsupported, Console, Driver};

/// The SQLSTATE of a cancelled statement (error 1317).
const CANCELLED_STATE: &str = "70100";

/// What a statement that ended the open transaction on its own (a DDL) is told.
fn implicit_commit(label: &str) -> String {
    format!("This statement committed the open transaction: {label} commits before DDL.")
}

/// The server answers a `KILL` for a connection that is already gone with this error.
const UNKNOWN_THREAD: u16 = 1094;

/// What one statement gave: the rows of its last result set, or only the rows it changed.
struct StatementOutput {
    /// The last result set that has columns
    shown: Option<(Vec<QueryColumn>, Vec<Vec<JsonValue>>)>,
    /// Rows changed by the result sets that have no columns
    affected: u64,
    notices: Vec<Notice>,
}

/// Runs one statement. A `CALL` may give several result sets; the last one with columns is kept.
async fn run_statement(conn: &mut Conn, label: &str, sql: &str) -> Result<StatementOutput, MysqlError> {
    let mut shown = None;
    let mut affected = 0;
    {
        let mut result = conn.query_iter(sql).await?;
        loop {
            let columns: Vec<Column> = result.columns_ref().to_vec();
            if columns.is_empty() {
                affected += result.affected_rows();
            }
            // Each call reads up to the end of the current result set
            let rows: Vec<Row> = result.collect().await?;
            if !columns.is_empty() {
                let info = columns.iter().map(|c| QueryColumn { name: c.name_str().to_string(), data_type: type_name(c) }).collect();
                let rows = rows
                    .into_iter()
                    .map(|row| row.unwrap().iter().zip(columns.iter()).map(|(value, column)| value_to_json(value, column)).collect())
                    .collect();
                shown = Some((info, rows));
            }
            if result.is_empty() {
                break;
            }
        }
    }
    let notices = if conn.get_warnings() > 0 { warnings(conn, label).await? } else { vec![] };
    Ok(StatementOutput { shown, affected, notices })
}

/// What `SHOW WARNINGS` says about the statement that just ran.
async fn warnings(conn: &mut Conn, label: &str) -> Result<Vec<Notice>, MysqlError> {
    let rows: Vec<(String, u32, String)> = conn.query("SHOW WARNINGS").await?;
    Ok(rows
        .into_iter()
        .map(|(level, code, message)| Notice { severity: level, message, detail: Some(format!("{label} warning {code}")), hint: None })
        .collect())
}

/// An error of the script as JSON for the UI, in the shape PostgreSQL's take. `statement` is
/// the (1-based index, total) of the failing statement when the script had more than one. An
/// error that isn't the server's (a lost connection) is just its message.
fn script_error(e: &MysqlError, endpoint: &MysqlEndpoint, statement: &Statement, position: Option<(usize, usize)>) -> String {
    let message = error_message(e, endpoint);
    let MysqlError::Server(server) = e else { return message };
    let (message, at) = if server.code == 1064 { locate_syntax_error(&message, statement) } else { (message, None) };
    serde_json::json!({
        "message": message,
        "severity": "ERROR",
        "detail": null,
        "hint": null,
        "position": at,
        "code": server.state,
        "statement": position.map(|(index, _)| index),
        "statementCount": position.map(|(_, total)| total),
    })
    .to_string()
}

fn cancelled_error(position: Option<(usize, usize)>) -> String {
    serde_json::json!({
        "message": CANCELLED,
        "severity": "ERROR",
        "detail": null,
        "hint": null,
        "position": null,
        "code": CANCELLED_STATE,
        "statement": position.map(|(index, _)| index),
        "statementCount": position.map(|(_, total)| total),
    })
    .to_string()
}

/// A syntax error says "… near 'TEXT' at line N", where TEXT is what follows the failing word
/// and N counts the lines of the statement. Returns the message without that line number and
/// the 1-based position, in characters of the whole script, of the failing word (the start of
/// line N when TEXT isn't found).
fn locate_syntax_error(message: &str, statement: &Statement) -> (String, Option<i64>) {
    let Some(line_at) = message.rfind(" at line ") else { return (message.to_string(), None) };
    let Ok(line) = message[line_at + " at line ".len()..].trim().parse::<usize>() else { return (message.to_string(), None) };
    let stripped = message[..line_at].to_string();

    let text: Vec<char> = statement.text.chars().collect();
    let line_start = statement.text.split('\n').take(line.saturating_sub(1)).map(|l| l.chars().count() + 1).sum::<usize>().min(text.len());
    let near: Option<Vec<char>> = message[..line_at]
        .find("near '")
        .and_then(|start| message[..line_at].strip_suffix('\'').map(|body| body[start + "near '".len()..].chars().collect()));
    let found = near.as_ref().and_then(|near| {
        if text.ends_with(near) {
            return Some(text.len() - near.len());
        }
        (line_start..=text.len().saturating_sub(near.len())).find(|&i| text[i..].starts_with(near))
    });
    (stripped, Some((statement.offset + found.unwrap_or(line_start) + 1) as i64))
}

/// Whether `sql` is the transaction ending on purpose: COMMIT or ROLLBACK.
fn ends_the_transaction(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    matches!(next_token(&chars, 0).map(|t| t.kind), Some(Kind::Word(w)) if w == "COMMIT" || w == "ROLLBACK")
}

/// Runs the statements in order, stopping at the first error or cancel.
async fn run_script(conn: &mut Conn, statements: &[Statement], endpoint: &MysqlEndpoint, cancelled: &AtomicBool) -> Result<QueryResult, String> {
    // A cancel that came while nothing was running is forgotten
    cancelled.store(false, Ordering::SeqCst);
    let total = statements.len();
    let start = Instant::now();
    let mut shown = None;
    let mut affected = 0;
    let mut notices = vec![];
    for (index, statement) in statements.iter().enumerate() {
        let position = (total > 1).then_some((index + 1, total));
        if cancelled.load(Ordering::SeqCst) {
            return Err(cancelled_error(position));
        }
        let was_open = in_transaction_status(conn);
        let output = run_statement(conn, endpoint.engine_label(), &statement.text).await.map_err(|e| script_error(&e, endpoint, statement, position))?;
        // A DDL commits the transaction before it runs; saying so beats a silent surprise
        if was_open && !in_transaction_status(conn) && !ends_the_transaction(&statement.text) {
            notices.push(Notice { severity: "Warning".into(), message: implicit_commit(endpoint.engine_label()), detail: None, hint: None });
        }
        // `KILL QUERY` ends some statements without an error (SLEEP gives 1)
        if cancelled.load(Ordering::SeqCst) {
            return Err(cancelled_error(position));
        }
        notices.extend(output.notices);
        match output.shown {
            Some(set) => shown = Some(set),
            None => affected += output.affected,
        }
    }
    let duration = start.elapsed().as_millis() as u64;
    let in_transaction = in_transaction_status(conn);
    Ok(match shown {
        Some((columns, rows)) => QueryResult {
            row_count: rows.len(),
            columns,
            rows,
            rows_affected: None,
            duration,
            statement_count: total,
            notices,
            in_transaction,
        },
        None => QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: affected as usize,
            rows_affected: Some(affected),
            duration,
            statement_count: total,
            notices,
            in_transaction,
        },
    })
}

#[async_trait]
impl Console for ServerDriver {
    /// Runs the script's statements in order on the editor's connection. The rows of the last
    /// statement that returns rows are shown; the others only add to the affected count.
    async fn execute(&self, sql: &str) -> Result<QueryResult, String> {
        let statements = split_mysql(sql)?;
        if statements.is_empty() {
            return Err("SQL query is required".into());
        }
        let endpoint = self.endpoint().clone();
        let cancelled = self.editor().cancel_flag();
        self.editor().run(move |conn| Box::pin(async move { run_script(conn, &statements, &endpoint, &cancelled).await })).await
    }

    /// Stops what the editor's connection is running, and the rest of its script: the script
    /// is marked, and `KILL QUERY` goes through another connection. With nothing running it
    /// does nothing: the next script starts afresh.
    async fn cancel(&self) -> Result<(), String> {
        self.editor().cancel_flag().store(true, Ordering::SeqCst);
        let id = self.editor().connection_id();
        let mut conn = self.conn().await?;
        match conn.query_drop(format!("KILL QUERY {id}")).await {
            Err(MysqlError::Server(server)) if server.code == UNKNOWN_THREAD => Ok(()),
            result => result.map_err(|e| error_message(&e, self.endpoint())),
        }
    }

    /// `EXPLAIN FORMAT=TREE` for one statement, which doesn't run it; `EXPLAIN ANALYZE` runs
    /// it, so it takes only statements that read.
    async fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainResult, String> {
        let statements = split_mysql(sql)?;
        let statement = match statements.as_slice() {
            [] => return Err("SQL query is required".into()),
            [one] => one,
            _ => return Err("EXPLAIN works on one statement at a time. Select the one to explain.".into()),
        };
        if statement.text.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("explain")) {
            return Err("Leave out EXPLAIN: the plan is requested for you.".into());
        }
        let engine = self.flavor().engine();
        if analyze {
            if !self.capabilities().explain_analyze {
                return Err(unsupported(engine, "EXPLAIN ANALYZE"));
            }
            if !is_reading_statement(&statement.text) {
                return Err(format!("EXPLAIN ANALYZE runs only reading statements on {}.", engine.label()));
            }
        }
        self.flavor().explain(self, &statement.text, analyze).await
    }
}

#[cfg(test)]
mod server_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{json, Value as JsonValue};

    use crate::db::mysql_common::tests_support::{exec, label, query_one, TestDb};
    use crate::db::mysql_common::driver::ServerDriver;
    use crate::db::mysql_common::split::DELIMITER_IS_A_CLIENT_COMMAND;
    use crate::db::sqlite::errors::CANCELLED;
    use crate::db::{Driver, ExplainFormat};

    const SHOP: &str = include_str!("fixtures/shop.sql");

    async fn open() -> (TestDb, Arc<ServerDriver>) {
        let db = TestDb::new().await;
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        (db, driver)
    }

    async fn shop() -> (TestDb, Arc<ServerDriver>) {
        let db = TestDb::new().await;
        exec(&db.endpoint, SHOP).await;
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        (db, driver)
    }

    /// The JSON an error of the script comes as.
    fn error(raw: &str) -> JsonValue {
        serde_json::from_str(raw).unwrap_or_else(|_| panic!("not a JSON error: {raw}"))
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_script_runs_every_statement_and_counts_changes() {
        let (_db, d) = open().await;
        let console = d.console().expect("mysql has a console");
        let result = console.execute("CREATE TABLE z(a INT); INSERT INTO z VALUES (1),(2); SELECT * FROM z").await.unwrap();
        assert_eq!(result.statement_count, 3);
        assert_eq!(result.row_count, 2);
        assert_eq!((result.columns[0].name.as_str(), result.columns[0].data_type.as_str()), ("a", "int"));
        assert_eq!(result.rows, vec![vec![json!(1)], vec![json!(2)]]);
        assert!(!result.in_transaction);

        let changed = console.execute("UPDATE z SET a = a + 10").await.unwrap();
        assert_eq!(changed.rows_affected, Some(2));
        assert!(changed.columns.is_empty() && changed.rows.is_empty());
        assert_eq!(changed.row_count, 2);

        // The changes of every statement add up when none returns rows
        let several = console.execute("DELETE FROM z WHERE a = 11; INSERT INTO z VALUES (5)").await.unwrap();
        assert_eq!((several.rows_affected, several.statement_count), (Some(2), 2));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_procedure_is_created_and_called_from_one_script() {
        let (_db, d) = open().await;
        let script = "CREATE PROCEDURE p() BEGIN DECLARE n INT DEFAULT 0; WHILE n < 2 DO SET n = n + 1; END WHILE; SELECT n; END; CALL p()";
        let result = d.console().unwrap().execute(script).await.unwrap();
        assert_eq!(result.statement_count, 2);
        assert_eq!(result.rows, vec![vec![json!(2)]]);
        assert_eq!(result.columns[0].name, "n");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_call_with_several_result_sets_shows_the_last_one() {
        let (_db, d) = open().await;
        let script = "CREATE PROCEDURE two() BEGIN SELECT 1 AS first_set; SELECT 'b' AS second_set; END; CALL two()";
        let result = d.console().unwrap().execute(script).await.unwrap();
        assert_eq!(result.columns[0].name, "second_set");
        assert_eq!(result.rows, vec![vec![json!("b")]]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn delimiter_is_explained() {
        let (_db, d) = open().await;
        let err = d.console().unwrap().execute("DELIMITER //\nSELECT 1//").await.err().unwrap();
        assert_eq!(err, DELIMITER_IS_A_CLIENT_COMMAND);
        assert_eq!(d.console().unwrap().execute("  ").await.err().unwrap(), "SQL query is required");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn warnings_become_notices() {
        let (_db, d) = open().await;
        let result = d.console().unwrap().execute("SELECT CAST('1x' AS UNSIGNED)").await.unwrap();
        let notice = result.notices.first().expect("a warning");
        assert_eq!(notice.severity, "Warning");
        assert!(notice.message.contains("1x"), "{}", notice.message);
        let clean = d.console().unwrap().execute("SELECT 1").await.unwrap();
        assert!(clean.notices.is_empty());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn cancelling_stops_a_running_query_and_the_rest_of_the_script() {
        let (db, d) = open().await;
        let running = {
            let d = d.clone();
            tokio::spawn(async move { d.console().unwrap().execute("SELECT SLEEP(30); CREATE TABLE never(x INT)").await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        d.console().unwrap().cancel().await.unwrap();
        let finished = tokio::time::timeout(Duration::from_secs(5), running).await.expect("a cancelled script ends within 5s");
        let err = error(&finished.unwrap().err().expect("a cancelled script is an error"));
        assert_eq!(err["message"], CANCELLED);
        assert_eq!(err["statementCount"], 2);
        let created: Option<i64> = query_one(
            &db.endpoint,
            &format!("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = '{}' AND table_name = 'never'", db.name),
        )
        .await;
        assert_eq!(created, Some(0), "the statements after the cancel never ran");
        // The next script starts afresh
        let next = d.console().unwrap().execute("SELECT 1").await.unwrap();
        assert_eq!(next.rows, vec![vec![json!(1)]]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_cancel_with_nothing_running_does_not_stop_the_next_script() {
        let (_db, d) = open().await;
        d.console().unwrap().cancel().await.unwrap();
        let result = d.console().unwrap().execute("SELECT 1").await.unwrap();
        assert_eq!(result.rows, vec![vec![json!(1)]]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn syntax_errors_point_at_their_line() {
        let (_db, d) = open().await;
        let script = "SELECT 1;\nSELEC 2";
        let err = error(&d.console().unwrap().execute(script).await.err().unwrap());
        assert_eq!((err["statement"].as_u64(), err["statementCount"].as_u64()), (Some(2), Some(2)));
        assert_eq!(err["code"], "42000");
        assert!(err["message"].as_str().unwrap().contains("SQL syntax"), "{err}");
        let position = err["position"].as_u64().unwrap() as usize;
        let at: String = script.chars().skip(position - 1).take(5).collect();
        assert_eq!(at, "SELEC");

        // Inside a statement of several lines the position is the failing word's, in characters
        let script = "SELECT 'ação',\n  2 FROM FROM t";
        let err = error(&d.console().unwrap().execute(script).await.err().unwrap());
        let position = err["position"].as_u64().unwrap() as usize;
        let at: String = script.chars().skip(position - 1).take(6).collect();
        assert_eq!(at, "FROM t");
        assert!(!err["message"].as_str().unwrap().contains("at line"), "the server's line is relative to the statement: {err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn an_error_in_a_later_statement_names_it() {
        let (_db, d) = shop().await;
        let err = error(&d.console().unwrap().execute("SELECT 1; SELECT * FROM nope; SELECT 3").await.err().unwrap());
        assert_eq!((err["statement"].as_u64(), err["statementCount"].as_u64()), (Some(2), Some(3)));
        assert!(err["message"].as_str().unwrap().contains("nope"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn explain_and_analyze_return_a_tree() {
        let (_db, d) = shop().await;
        let console = d.console().unwrap();
        let plan = console.explain("SELECT * FROM customers WHERE id = 1", false).await.unwrap();
        assert_eq!((plan.format, plan.analyzed), (ExplainFormat::MysqlTree, false));
        let roots = plan.plan.as_array().unwrap();
        assert!(!roots.is_empty() && !roots[0]["label"].as_str().unwrap().is_empty());
        assert_eq!(roots[0]["actualRows"], json!(null));

        let analyzed = console.explain("SELECT * FROM customers", true).await.unwrap();
        assert!(analyzed.analyzed);
        let roots = analyzed.plan.as_array().unwrap();
        assert!(roots[0]["actualRows"].is_number() && roots[0]["loops"].is_number(), "{}", roots[0]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn analyze_refuses_a_writing_statement() {
        let (db, d) = shop().await;
        exec(&db.endpoint, "INSERT INTO customers (email, name) VALUES ('a@x.com', 'Ana')").await;
        let err = d.console().unwrap().explain("UPDATE customers SET name = 'Changed'", true).await.err().unwrap();
        assert_eq!(err, format!("EXPLAIN ANALYZE runs only reading statements on {}.", label()));
        let name: Option<String> = query_one(&db.endpoint, "SELECT name FROM customers").await;
        assert_eq!(name.as_deref(), Some("Ana"), "the statement never ran");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn explain_takes_one_statement_without_explain() {
        let (_db, d) = shop().await;
        let console = d.console().unwrap();
        assert_eq!(console.explain("", false).await.err().unwrap(), "SQL query is required");
        assert_eq!(
            console.explain("SELECT 1; SELECT 2", false).await.err().unwrap(),
            "EXPLAIN works on one statement at a time. Select the one to explain."
        );
        assert_eq!(console.explain("EXPLAIN SELECT 1", false).await.err().unwrap(), "Leave out EXPLAIN: the plan is requested for you.");
    }
}
