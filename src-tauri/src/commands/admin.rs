//! The server around the database: other databases, schemas, roles and activity.

use tauri::State;

use super::connect::ConnectResponse;
use super::require;
use super::state::AppState;
use crate::db::model::{DatabaseInfo, RoleInfo, ServerActivity};

#[tauri::command]
pub async fn get_databases(state: State<'_, AppState>, session_id: String) -> Result<Vec<DatabaseInfo>, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.databases().await
}

/// Opens a new session on another database of the same server, with the same login,
/// SSL, read-only and SSH settings as `session_id`.
#[tauri::command]
pub async fn connect_database(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> Result<ConnectResponse, String> {
    open_database_session(&state, &session_id, &database).await
}

/// Opens `database` from the server of `session_id` and registers it as a new session.
pub(crate) async fn open_database_session(
    state: &AppState,
    session_id: &str,
    database: &str,
) -> Result<ConnectResponse, String> {
    let driver = state.driver(session_id).await?;
    let opened = require(driver.admin(), driver.engine(), "Server administration")?
        .open_database(database)
        .await?;
    let session_id = state.register(opened.clone()).await?;
    Ok(ConnectResponse::new(session_id, opened.as_ref()))
}

#[tauri::command]
pub async fn create_database(state: State<'_, AppState>, session_id: String, name: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.create_database(&name).await
}

#[tauri::command]
pub async fn drop_database(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    force: bool,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.drop_database(&name, force).await
}

#[tauri::command]
pub async fn create_schema(state: State<'_, AppState>, session_id: String, name: String) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.create_schema(&name).await
}

#[tauri::command]
pub async fn rename_schema(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    new_name: String,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.rename_schema(&name, &new_name).await
}

#[tauri::command]
pub async fn drop_schema(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    cascade: bool,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.drop_schema(&name, cascade).await
}

#[tauri::command]
pub async fn get_roles(state: State<'_, AppState>, session_id: String) -> Result<Vec<RoleInfo>, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.roles().await
}

#[tauri::command]
pub async fn get_server_activity(
    state: State<'_, AppState>,
    session_id: String,
    all_databases: bool,
) -> Result<ServerActivity, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.activity(all_databases).await
}

/// Cancels the session's running query (`terminate` false) or ends the session.
/// Refuses DBunny's own main connection, which would cut off the app.
#[tauri::command]
pub async fn signal_backend(
    state: State<'_, AppState>,
    session_id: String,
    pid: i64,
    terminate: bool,
) -> Result<bool, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.admin(), driver.engine(), "Server administration")?.signal_backend(pid, terminate).await
}
