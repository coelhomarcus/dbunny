//! Changing tables and other objects: the Structure tab, the table builder and the
//! explorer's rename, drop, truncate, duplicate and refresh.

use tauri::State;

use super::require;
use super::state::AppState;
use crate::db::model::{NewTable, StructureChange};

/// The SQL a change would run, for showing before running it.
#[tauri::command]
pub async fn preview_structure_change(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    change: StructureChange,
) -> Result<String, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .preview_change(&schema, &table, &change)
        .await
}

#[tauri::command]
pub async fn apply_structure_change(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    change: StructureChange,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .apply_change(&schema, &table, change)
        .await
}

#[tauri::command]
pub async fn preview_create_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    spec: NewTable,
) -> Result<String, String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .preview_create_table(&schema, &table, &spec)
        .await
}

#[tauri::command]
pub async fn create_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    spec: NewTable,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .create_table(&schema, &table, spec)
        .await
}

#[tauri::command]
pub async fn rename_relation(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
    new_name: String,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .rename_relation(&schema, &name, &kind, &new_name)
        .await
}

#[tauri::command]
pub async fn drop_relation(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
    cascade: bool,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .drop_relation(&schema, &name, &kind, cascade)
        .await
}

#[tauri::command]
pub async fn truncate_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    restart_identity: bool,
    cascade: bool,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .truncate_table(&schema, &name, restart_identity, cascade)
        .await
}

#[tauri::command]
pub async fn refresh_materialized_view(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .refresh_materialized_view(&schema, &name)
        .await
}

/// A copy with the same columns, defaults, constraints, indexes and comments, optionally
/// with the rows.
#[tauri::command]
pub async fn duplicate_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    new_name: String,
    with_data: bool,
) -> Result<(), String> {
    let driver = state.driver(&session_id).await?;
    require(driver.schema_editor(), driver.engine(), "Structure editing")?
        .duplicate_table(&schema, &name, &new_name, with_data)
        .await
}
