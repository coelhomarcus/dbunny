//! A transaction opened from the query editor and kept open across runs.

use tauri::State;

use super::require;
use super::state::AppState;

/// Opens a transaction every editor run joins until commit or rollback.
#[tauri::command]
pub async fn begin_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.transactions(), driver.engine(), "Transactions")?.begin().await
}

#[tauri::command]
pub async fn commit_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.transactions(), driver.engine(), "Transactions")?.commit().await
}

#[tauri::command]
pub async fn rollback_transaction(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.transactions(), driver.engine(), "Transactions")?.rollback().await
}

#[tauri::command]
pub async fn transaction_open(state: State<'_, AppState>, session_id: String) -> Result<bool, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.transactions(), driver.engine(), "Transactions")?.is_open().await
}
