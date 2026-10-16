//! Query editor extras: cancelling a running query, a transaction kept open across
//! runs, and EXPLAIN plans.

use std::sync::Arc;

use serde_json::Value as JsonValue;
use tauri::State;
use tokio_postgres::error::SqlState;
use tokio_postgres::{Client, SimpleQueryMessage};

use super::query::{db_message, pg_error_in_script};
use super::sqlsplit::split_statements;
use super::state::AppState;
use crate::db::model::ExplainResult;
use crate::db::postgres::connect::open_connection;
use crate::db::postgres::{Link, Notices};

/// Asks the server to stop whatever the query editor's connection is running.
#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    // Nothing open means nothing running
    let Some(client) = state.postgres(&session_id).await?.running_editor().await? else {
        return Ok(());
    };
    let link = state.postgres(&session_id).await?.link().await?;
    client.cancel_token().cancel_query(link.tls).await.map_err(|e| e.to_string())
}

/// Opens a transaction on a connection of its own: every editor run joins it until
/// commit or rollback, while table views keep using the main connection.
#[tauri::command]
pub async fn begin_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    if state.postgres(&session_id).await?.in_transaction().await? {
        return Err("A transaction is already open".into());
    }
    let link = state.postgres(&session_id).await?.link().await?;
    let notices = state.postgres(&session_id).await?.notices().await?;
    let client = start_transaction(&link, &notices).await?;

    let driver = state.postgres(&session_id).await?;
    let mut pg = driver.lock().await;
    let session = pg.as_mut().ok_or("Session not found")?;
    if session.transaction.as_ref().is_some_and(|tx| !tx.is_closed()) {
        return Err("A transaction is already open".into());
    }
    session.transaction = Some(Arc::new(client));
    Ok(())
}

async fn start_transaction(link: &Link, notices: &Notices) -> Result<Client, String> {
    let client = open_connection(link, notices)
        .await
        .map_err(|e| format!("Couldn't open the transaction connection: {}", e))?;
    client.batch_execute("BEGIN").await.map_err(|e| db_message(&e))?;
    Ok(client)
}

async fn take_transaction(state: &AppState, session_id: &str) -> Result<Arc<Client>, String> {
    let driver = state.postgres(session_id).await?;
    let mut pg = driver.lock().await;
    let session = pg.as_mut().ok_or("Session not found")?;
    session.transaction.take().ok_or_else(|| "No transaction is open".to_string())
}

/// Commits, unless a statement failed: Postgres would silently roll back on COMMIT
/// then, so that case is reported as an error. The connection closes either way.
pub(crate) async fn finish_commit(client: &Client) -> Result<(), String> {
    if let Err(e) = client.simple_query("SELECT 1").await {
        if e.code() == Some(&SqlState::IN_FAILED_SQL_TRANSACTION) {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err("A statement in the transaction failed, so it was rolled back instead of committed".into());
        }
        return Err(db_message(&e));
    }
    client
        .batch_execute("COMMIT")
        .await
        .map_err(|e| format!("Commit failed, nothing was saved: {}", db_message(&e)))
}

#[tauri::command]
pub async fn commit_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let client = take_transaction(&state, &session_id).await?;
    finish_commit(&client).await
}

/// Rolls back. Closing the connection would roll back too, so a broken connection isn't an error.
#[tauri::command]
pub async fn rollback_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let client = take_transaction(&state, &session_id).await?;
    let _ = client.batch_execute("ROLLBACK").await;
    Ok(())
}

#[tauri::command]
pub async fn transaction_open(state: State<'_, AppState>, session_id: String) -> Result<bool, String> {
    state.postgres(&session_id).await?.in_transaction().await
}

const SAVEPOINT: &str = "dbunny_explain";

/// EXPLAIN for one statement. With `analyze` the statement really runs, so it's wrapped
/// in a transaction that is always rolled back: inside the editor's open transaction via
/// a savepoint, otherwise on a short-lived connection so nothing else can slip into it.
#[tauri::command]
pub async fn explain_query(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
    analyze: bool,
) -> Result<ExplainResult, String> {
    let statements = split_statements(&sql);
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

    let (client, in_transaction) = state.postgres(&session_id).await?.editor_client().await?;
    let messages = if !analyze {
        client.simple_query(&explain_sql).await.map_err(error)?
    } else if in_transaction {
        client.batch_execute(&format!("SAVEPOINT {SAVEPOINT}")).await.map_err(|e| db_message(&e))?;
        let result = client.simple_query(&explain_sql).await;
        let _ = client.batch_execute(&format!("ROLLBACK TO SAVEPOINT {SAVEPOINT}")).await;
        result.map_err(error)?
    } else {
        let link = state.postgres(&session_id).await?.link().await?;
        let notices = state.postgres(&session_id).await?.notices().await?;
        let scratch = start_transaction(&link, &notices).await?;
        let result = scratch.simple_query(&explain_sql).await;
        let _ = scratch.batch_execute("ROLLBACK").await;
        result.map_err(error)?
    };

    Ok(ExplainResult { plan: plan_from_messages(&messages)?, analyzed: analyze })
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

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::db::postgres::connect::connection_config;

    fn link() -> Link {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let url = url::Url::parse(&url).unwrap();
        let (config, tls) = connection_config(&url).unwrap();
        Link { config, tls }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn read_only_connections_reject_writes_on_every_connection() {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL").unwrap();
        let separator = if url.contains('?') { '&' } else { '?' };
        let url = url::Url::parse(&format!("{url}{separator}options=-c%20default_transaction_read_only%3Don")).unwrap();
        let (config, tls) = connection_config(&url).unwrap();
        let link = Link { config, tls };
        let notices: Notices = Default::default();

        let main = open_connection(&link, &notices).await.unwrap();
        let shown: String = main.query_one("SHOW transaction_read_only", &[]).await.unwrap().get(0);
        assert_eq!(shown, "on");
        let err = main.batch_execute("CREATE TEMP TABLE nope (v int)").await.unwrap_err();
        assert_eq!(err.code(), Some(&SqlState::READ_ONLY_SQL_TRANSACTION));

        // The editor's transaction connection is opened from the same link, so it's read-only too
        let tx = start_transaction(&link, &notices).await.unwrap();
        let err = tx.batch_execute("CREATE TEMP TABLE nope (v int)").await.unwrap_err();
        assert_eq!(err.code(), Some(&SqlState::READ_ONLY_SQL_TRANSACTION));
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
    async fn a_transaction_connection_is_invisible_to_others_until_commit() {
        let link = link();
        let notices: Notices = Default::default();
        let main = open_connection(&link, &notices).await.unwrap();
        main.batch_execute("DROP TABLE IF EXISTS dbunny_tx_test; CREATE TABLE dbunny_tx_test (v int)").await.unwrap();

        let tx = start_transaction(&link, &notices).await.unwrap();
        tx.batch_execute("INSERT INTO dbunny_tx_test VALUES (1)").await.unwrap();
        let outside: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(outside, 0, "uncommitted rows must not be visible");
        finish_commit(&tx).await.unwrap();
        let after: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(after, 1);

        // A failed statement turns COMMIT into an error, and nothing is saved
        let tx = start_transaction(&link, &notices).await.unwrap();
        tx.batch_execute("INSERT INTO dbunny_tx_test VALUES (2)").await.unwrap();
        assert!(tx.batch_execute("SELECT 1/0").await.is_err());
        let err = finish_commit(&tx).await.unwrap_err();
        assert!(err.contains("rolled back"), "{err}");
        let total: i64 = main.query_one("SELECT count(*) FROM dbunny_tx_test", &[]).await.unwrap().get(0);
        assert_eq!(total, 1);

        main.batch_execute("DROP TABLE dbunny_tx_test").await.unwrap();
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
