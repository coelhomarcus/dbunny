//! The SQL console: running scripts, cancelling them and EXPLAIN plans.

use tauri::State;

use super::require;
use super::state::AppState;
use crate::db::model::{ExplainResult, QueryResult};

#[tauri::command]
pub async fn execute_query(state: State<'_, AppState>, session_id: String, sql: String) -> Result<QueryResult, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.console(), driver.engine(), "The SQL console")?.execute(&sql).await
}

/// Asks the server to stop whatever the query editor is running.
#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.console(), driver.engine(), "The SQL console")?.cancel().await
}

/// EXPLAIN for one statement; with `analyze` it really runs, and is always rolled back.
#[tauri::command]
pub async fn explain_query(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
    analyze: bool,
) -> Result<ExplainResult, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.console(), driver.engine(), "The SQL console")?.explain(&sql, analyze).await
}
