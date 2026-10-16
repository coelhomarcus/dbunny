//! The SQL console: running the editor's scripts, cancelling them and EXPLAIN plans.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio_postgres::types::Type;
use tokio_postgres::SimpleQueryMessage;

use super::data::{columns_from_statement, db_message, row_to_json_values, StatementOutput};
use super::text::{is_native, text_to_json};
use super::transactions::start_transaction;
use super::PostgresDriver;
use crate::db::model::{ExplainResult, QueryResult};
use crate::db::sql::split::split_statements;
use crate::db::{Console, ExplainFormat};

/// Error as JSON for the UI. `offset` shifts the position to be relative to the whole
/// script, and `statement` is the (1-based index, total) of the failing statement
/// when the script had more than one.
fn pg_error_in_script(e: tokio_postgres::Error, offset: i64, statement: Option<(usize, usize)>) -> String {
    if let Some(db) = e.as_db_error() {
        let position = match db.position() {
            Some(tokio_postgres::error::ErrorPosition::Original(p)) => Some((*p as i64 + offset).max(1)),
            _ => None,
        };
        serde_json::json!({
            "message": db.message(),
            "severity": db.severity(),
            "detail": db.detail(),
            "hint": db.hint(),
            "position": position,
            "code": db.code().code(),
            "statement": statement.map(|(index, _)| index),
            "statementCount": statement.map(|(_, total)| total),
        })
        .to_string()
    } else {
        e.to_string()
    }
}

/// Runs a single statement and returns its rows as JSON.
///
/// Types the binary decoders know are read directly. If any column has another type
/// (enum, interval, inet, money, range, domain...), the statement runs through the
/// text protocol and each value is converted from its text form.
pub(super) async fn run_statement(
    client: &tokio_postgres::Client,
    sql: &str,
) -> Result<StatementOutput, tokio_postgres::Error> {
    let stmt = client.prepare(sql).await?;

    if stmt.columns().is_empty() {
        let affected = client.execute(&stmt, &[]).await?;
        return Ok(StatementOutput {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(affected),
        });
    }

    let columns = columns_from_statement(stmt.columns());

    let rows = if stmt.columns().iter().all(|c| is_native(c.type_())) {
        let rows = client.query(&stmt, &[]).await?;
        rows.iter().map(row_to_json_values).collect()
    } else {
        let types: Vec<Type> = stmt.columns().iter().map(|c| c.type_().clone()).collect();
        let messages = client.simple_query(sql).await?;
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

    Ok(StatementOutput {
        columns,
        rows,
        rows_affected: None,
    })
}

/// Rows of a change shown in the editor; the count of every changed row still comes back.
const CHANGED_ROWS_SHOWN: usize = 500;
const CHANGED_PREFIX: &str = "WITH dbunny_changed AS (\n";

/// The first word of a statement, past comments.
fn leading_keyword(sql: &str) -> String {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else {
            break;
        }
    }
    rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase()
}

/// An INSERT, UPDATE or DELETE rewritten to also return the rows it changed: the statement
/// still runs once, the server counts every changed row and sends back the first ones.
/// None for anything else, or when the statement already says what to return (RETURNING)
/// or brings its own WITH, which this can't nest safely.
fn with_changed_rows(sql: &str) -> Option<String> {
    if !matches!(leading_keyword(sql).as_str(), "insert" | "update" | "delete") {
        return None;
    }
    let lower = sql.to_ascii_lowercase();
    let has_word = |word: &str| {
        lower.match_indices(word).any(|(i, _)| {
            let before = lower[..i].chars().next_back();
            let after = lower[i + word.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    };
    if has_word("returning") {
        return None;
    }
    // On new lines, so a comment at the end of the statement can't swallow what follows
    Some(format!(
        "{CHANGED_PREFIX}{sql}\nRETURNING *\n) SELECT (SELECT count(*) FROM dbunny_changed) AS dbunny_total, * FROM dbunny_changed LIMIT {CHANGED_ROWS_SHOWN}"
    ))
}

/// The rewritten statement's output as the changed rows: the count column goes, and the
/// count becomes the number of rows affected.
fn changed_rows_output(mut output: StatementOutput) -> StatementOutput {
    let total = output
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0);
    output.columns.remove(0);
    for row in &mut output.rows {
        row.remove(0);
    }
    output.rows_affected = Some(total);
    output
}

/// Runs one statement of the editor's script, with the changed rows of an INSERT, UPDATE
/// or DELETE. A rewrite the server can't take (an insert into a view whose rules don't
/// return rows, say) is refused before anything runs, and then the statement runs as typed.
async fn run_editor_statement(
    client: &tokio_postgres::Client,
    sql: &str,
    offset: i64,
    in_transaction: bool,
    position: Option<(usize, usize)>,
) -> Result<StatementOutput, String> {
    if let Some(rewritten) = with_changed_rows(sql) {
        match run_statement(client, &rewritten).await {
            Ok(output) => return Ok(changed_rows_output(output)),
            // In a transaction the failure has aborted it, so there's no second try
            Err(e) if !in_transaction && e.code() == Some(&tokio_postgres::error::SqlState::FEATURE_NOT_SUPPORTED) => {}
            Err(e) => return Err(pg_error_in_script(e, offset - CHANGED_PREFIX.chars().count() as i64, position)),
        }
    }
    run_statement(client, sql).await.map_err(|e| pg_error_in_script(e, offset, position))
}

const SAVEPOINT: &str = "dbunny_explain";

#[async_trait]
impl Console for PostgresDriver {
    async fn execute(&self, sql: &str) -> Result<QueryResult, String> {
        let statements = split_statements(sql);
        if statements.is_empty() {
            return Err("SQL query is required".into());
        }

        let (client, in_transaction) = self.editor_client().await?;
        let notices = self.notices().await?;
        // Only this run's notices are returned
        notices.lock().unwrap().clear();
        let total = statements.len();

        let start = std::time::Instant::now();
        // The result set of the last statement that returns rows is the one shown;
        // statements without rows only add to the affected count.
        let mut shown: Option<StatementOutput> = None;
        let mut affected: u64 = 0;
        for (index, statement) in statements.iter().enumerate() {
            let position = (total > 1).then_some((index + 1, total));
            let output = run_editor_statement(&client, &statement.text, statement.offset as i64, in_transaction, position).await?;
            match output.rows_affected {
                // A change that brought its rows back is shown, and still counts as affected
                Some(count) if !output.columns.is_empty() => {
                    affected += count;
                    shown = Some(output);
                }
                Some(count) => affected += count,
                None => shown = Some(output),
            }
        }
        let duration = start.elapsed().as_millis() as u64;
        let notices = std::mem::take(&mut *notices.lock().unwrap());

        Ok(match shown {
            Some(output) => QueryResult {
                row_count: output.rows.len(),
                columns: output.columns,
                rows: output.rows,
                // The changed rows on show: the count of all of them
                rows_affected: output.rows_affected,
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

    /// Asks the server to stop whatever the query editor's connection is running.
    async fn cancel(&self) -> Result<(), String> {
        // Nothing open means nothing running
        let Some(client) = self.running_editor().await? else {
            return Ok(());
        };
        let link = self.link().await?;
        client.cancel_token().cancel_query(link.tls).await.map_err(|e| e.to_string())
    }

    /// EXPLAIN for one statement. With `analyze` the statement really runs, so it's wrapped
    /// in a transaction that is always rolled back: inside the editor's open transaction via
    /// a savepoint, otherwise on a short-lived connection so nothing else can slip into it.
    async fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainResult, String> {
        let statements = split_statements(sql);
        let statement = match statements.as_slice() {
            [] => return Err("SQL query is required".into()),
            [one] => one,
            _ => return Err("EXPLAIN works on one statement at a time. Select the one to explain.".into()),
        };
        if statement.text.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("explain")) {
            return Err("Leave out EXPLAIN: the plan is requested for you.".into());
        }

        let prefix = if analyze { "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) " } else { "EXPLAIN (FORMAT JSON) " };
        let explain_sql = format!("{prefix}{}", statement.text);
        // Positions in errors point into explain_sql; move them back into the user's script
        let shift = statement.offset as i64 - prefix.chars().count() as i64;
        let error = |e: tokio_postgres::Error| pg_error_in_script(e, shift, None);

        let (client, in_transaction) = self.editor_client().await?;
        let messages = if !analyze {
            client.simple_query(&explain_sql).await.map_err(error)?
        } else if in_transaction {
            client.batch_execute(&format!("SAVEPOINT {SAVEPOINT}")).await.map_err(|e| db_message(&e))?;
            let result = client.simple_query(&explain_sql).await;
            let _ = client.batch_execute(&format!("ROLLBACK TO SAVEPOINT {SAVEPOINT}")).await;
            result.map_err(error)?
        } else {
            let link = self.link().await?;
            let notices = self.notices().await?;
            let scratch = start_transaction(&link, &notices).await?;
            let result = scratch.simple_query(&explain_sql).await;
            let _ = scratch.batch_execute("ROLLBACK").await;
            result.map_err(error)?
        };

        Ok(ExplainResult { plan: plan_from_messages(&messages)?, analyzed: analyze, format: ExplainFormat::PostgresJson })
    }
}

fn plan_from_messages(messages: &[SimpleQueryMessage]) -> Result<JsonValue, String> {
    let text: String = messages
        .iter()
        .filter_map(|m| match m {
            SimpleQueryMessage::Row(row) => row.get(0).map(str::to_string),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&text).map_err(|e| format!("Unexpected EXPLAIN output: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_changes_get_their_rows_back() {
        assert!(with_changed_rows("UPDATE t SET a = 1 WHERE id = 7").is_some());
        assert!(with_changed_rows("  -- fix it\n/* note */ delete from t").is_some());
        assert!(with_changed_rows("insert into t values (1)").is_some());
        assert!(with_changed_rows("SELECT 1").is_none());
        assert!(with_changed_rows("WITH x AS (SELECT 1) UPDATE t SET a = 1").is_none(), "its own WITH");
        assert!(with_changed_rows("UPDATE t SET a = 1 RETURNING id").is_none(), "already returns rows");
        assert!(with_changed_rows("update t set returning_count = 1").is_some(), "a column named like the keyword");
        let sql = with_changed_rows("UPDATE t SET a = 1 -- trailing comment").unwrap();
        assert!(sql.contains("-- trailing comment\nRETURNING *\n)"), "{sql}");
    }
}

/// Tests that need a real server. Run them with:
///   DBUNNY_TEST_DATABASE_URL=postgresql://user:pass@localhost/db cargo test --lib -- --ignored
/// Everything they create is temporary (pg_temp) and disappears with the connection.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::db::postgres::connect::{connection_config, open_connection};
    use crate::db::postgres::{Link, Notices};
    use serde_json::json;
    use std::sync::Arc;
    use tokio_postgres::error::SqlState;

    async fn connect() -> tokio_postgres::Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .expect("could not connect to the test database");
        tokio::spawn(connection);
        client
    }

    async fn run(client: &tokio_postgres::Client, sql: &str) -> StatementOutput {
        run_statement(client, sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"))
    }

    fn link() -> Link {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let url = url::Url::parse(&url).unwrap();
        let (config, tls) = connection_config(&url).unwrap();
        Link { config, tls }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn unusual_types_are_readable_instead_of_binary() {
        let client = connect().await;
        client.batch_execute("CREATE TYPE pg_temp.mood AS ENUM ('sad', 'happy')").await.unwrap();
        client.batch_execute("CREATE DOMAIN pg_temp.posint AS int CHECK (VALUE > 0)").await.unwrap();

        let out = run(
            &client,
            "SELECT 'happy'::pg_temp.mood, 5::pg_temp.posint, ARRAY['sad','happy']::pg_temp.mood[], \
             int4range(1, 10), '1 day 02:00:00'::interval, '10.0.0.1/24'::inet, '10:00:00+02'::timetz, \
             ARRAY[1,2]::int4[], true, 7::int8, NULL::interval, 'abc'::bytea",
        )
        .await;

        assert_eq!(out.rows.len(), 1);
        assert_eq!(
            out.rows[0],
            vec![
                json!("happy"),
                json!(5),
                json!(["sad", "happy"]),
                json!("[1,10)"),
                json!("1 day 02:00:00"),
                json!("10.0.0.1/24"),
                json!("10:00:00+02"),
                json!([1, 2]),
                json!(true),
                json!(7),
                JsonValue::Null,
                json!("YWJj"),
            ]
        );
        assert_eq!(out.columns[0].data_type, "mood");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn native_types_still_use_the_binary_path() {
        let client = connect().await;
        let out = run(&client, "SELECT 1::int4, 'a'::text, 1.5::float8, '{\"a\":1}'::jsonb, ARRAY['x','y']::text[]").await;
        assert_eq!(
            out.rows[0],
            vec![json!(1), json!("a"), json!(1.5), json!({"a": 1}), json!(["x", "y"])]
        );
        assert_eq!(out.columns[0].data_type, "int4");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn insert_returning_gives_rows_and_plain_insert_gives_a_count() {
        let client = connect().await;
        client.batch_execute("CREATE TEMP TABLE returning_test (id int)").await.unwrap();

        let returned = run(&client, "INSERT INTO returning_test VALUES (1), (2) RETURNING id").await;
        assert_eq!(returned.rows, vec![vec![json!(1)], vec![json!(2)]]);
        assert_eq!(returned.rows_affected, None);

        let plain = run(&client, "INSERT INTO returning_test VALUES (3)").await;
        assert!(plain.rows.is_empty());
        assert_eq!(plain.rows_affected, Some(1));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_table_with_an_enum_and_nulls_reads_back_correctly() {
        let client = connect().await;
        client.batch_execute("CREATE TYPE pg_temp.size AS ENUM ('s', 'm', 'l')").await.unwrap();
        client
            .batch_execute("CREATE TEMP TABLE shirts (id int PRIMARY KEY, size pg_temp.size, tags text[], note text)")
            .await
            .unwrap();
        client
            .batch_execute("INSERT INTO shirts VALUES (1, 'm', ARRAY['a','b'], NULL), (2, NULL, NULL, 'x')")
            .await
            .unwrap();

        let out = run(&client, "SELECT * FROM shirts ORDER BY id").await;
        assert_eq!(out.rows[0], vec![json!(1), json!("m"), json!(["a", "b"]), JsonValue::Null]);
        assert_eq!(out.rows[1], vec![json!(2), JsonValue::Null, JsonValue::Null, json!("x")]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_script_runs_each_statement_in_order() {
        let client = connect().await;
        let script = "CREATE TEMP TABLE script_test (id int); \
                      INSERT INTO script_test VALUES (1), (2); \
                      INSERT INTO script_test VALUES (3); \
                      SELECT count(*)::int AS n FROM script_test";
        let mut shown = None;
        for statement in split_statements(script) {
            let out = run(&client, &statement.text).await;
            if out.rows_affected.is_none() {
                shown = Some(out);
            }
        }
        assert_eq!(shown.unwrap().rows, vec![vec![json!(3)]]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn semicolons_in_strings_and_function_bodies_survive_on_a_real_server() {
        let client = connect().await;
        let script = "CREATE FUNCTION pg_temp.two() RETURNS int AS $$ BEGIN PERFORM 1; RETURN 2; END; $$ LANGUAGE plpgsql; \
                      SELECT 'a;b' AS s, pg_temp.two() AS n";
        let statements = split_statements(script);
        assert_eq!(statements.len(), 2);
        run(&client, &statements[0].text).await;
        let out = run(&client, &statements[1].text).await;
        assert_eq!(out.rows, vec![vec![json!("a;b"), json!(2)]]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn error_positions_are_shifted_to_the_whole_script() {
        let client = connect().await;
        let script = "SELECT 1;\n  SELECT nope";
        let statements = split_statements(script);
        let err = run_statement(&client, &statements[1].text)
            .await
            .err()
            .expect("the second statement should fail");
        let json: serde_json::Value = serde_json::from_str(&pg_error_in_script(err, statements[1].offset as i64, Some((2, 2)))).unwrap();
        let position = json["position"].as_u64().unwrap() as usize;
        // The position points at "nope", counted in characters from the start of the script
        let at_position: String = script.chars().skip(position - 1).take(4).collect();
        assert_eq!(at_position, "nope");
        assert_eq!(json["statement"], 2);
        assert_eq!(json["statementCount"], 2);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn changes_in_the_editor_show_the_rows_they_changed() {
        let client = connect().await;
        client.batch_execute("CREATE TEMP TABLE stock (id int PRIMARY KEY, qty int); INSERT INTO stock SELECT g, 0 FROM generate_series(1, 600) g").await.unwrap();
        let run = |sql: &'static str| {
            let client = &client;
            async move { run_editor_statement(client, sql, 0, false, None).await }
        };

        let out = run("UPDATE stock SET qty = 9 WHERE id = 7 -- one row").await.unwrap();
        assert_eq!(out.rows_affected, Some(1));
        assert_eq!(out.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["id", "qty"]);
        assert_eq!(out.rows, vec![vec![json!(7), json!(9)]]);

        // Every row is changed and counted; the first ones come back
        let out = run("DELETE FROM stock WHERE id > 50").await.unwrap();
        assert_eq!(out.rows_affected, Some(550));
        assert_eq!(out.rows.len(), CHANGED_ROWS_SHOWN.min(550));
        let left: i64 = client.query_one("SELECT count(*) FROM stock", &[]).await.unwrap().get(0);
        assert_eq!(left, 50, "the statement ran once, in full");

        // Nothing matched: no rows, zero affected
        let out = run("UPDATE stock SET qty = 1 WHERE id = -1").await.unwrap();
        assert_eq!((out.rows_affected, out.rows.len()), (Some(0), 0));

        // RETURNING of its own is left as typed
        let out = run("INSERT INTO stock VALUES (1000, 5) RETURNING id").await.unwrap();
        assert_eq!((out.rows_affected, out.rows.clone()), (None, vec![vec![json!(1000)]]));

        // Errors still point into the statement as typed
        let err = run("UPDATE stock SET nope = 1").await.err().expect("an unknown column fails");
        let error: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(error["position"], json!(18), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn notices_are_collected_from_the_connection() {
        let notices: Notices = Default::default();
        let client = open_connection(&link(), &notices).await.unwrap();
        client
            .batch_execute("DO $$ BEGIN RAISE NOTICE 'hello %', 42; RAISE WARNING 'careful' USING HINT = 'look'; END $$")
            .await
            .unwrap();
        let list = notices.lock().unwrap().clone();
        assert_eq!(list.len(), 2, "{list:?}");
        assert_eq!((list[0].severity.as_str(), list[0].message.as_str()), ("NOTICE", "hello 42"));
        assert_eq!((list[1].severity.as_str(), list[1].hint.as_deref()), ("WARNING", Some("look")));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn cancelling_stops_a_running_query() {
        let link = link();
        let notices: Notices = Default::default();
        let client = Arc::new(open_connection(&link, &notices).await.unwrap());
        let running = {
            let client = Arc::clone(&client);
            tokio::spawn(async move { client.batch_execute("SELECT pg_sleep(10)").await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let started = std::time::Instant::now();
        client.cancel_token().cancel_query(link.tls.clone()).await.unwrap();
        let err = running.await.unwrap().unwrap_err();
        assert_eq!(err.code(), Some(&SqlState::QUERY_CANCELED));
        assert!(started.elapsed().as_secs() < 3);
        // The connection is still usable afterwards
        client.batch_execute("SELECT 1").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn explain_analyze_runs_the_statement_and_rolls_it_back() {
        let link = link();
        let notices: Notices = Default::default();
        let main = open_connection(&link, &notices).await.unwrap();
        main.batch_execute("DROP TABLE IF EXISTS dbunny_explain_test; CREATE TABLE dbunny_explain_test (v int)").await.unwrap();

        let scratch = start_transaction(&link, &notices).await.unwrap();
        let messages = scratch
            .simple_query("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) INSERT INTO dbunny_explain_test SELECT generate_series(1, 5)")
            .await
            .unwrap();
        scratch.batch_execute("ROLLBACK").await.unwrap();
        let plan = plan_from_messages(&messages).unwrap();
        assert_eq!(plan[0]["Plan"]["Node Type"], "ModifyTable");
        assert!(plan[0]["Execution Time"].is_number());

        let count: i64 = main.query_one("SELECT count(*) FROM dbunny_explain_test", &[]).await.unwrap().get(0);
        assert_eq!(count, 0, "EXPLAIN ANALYZE must not keep its changes");
        main.batch_execute("DROP TABLE dbunny_explain_test").await.unwrap();
    }
}
