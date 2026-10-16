//! A table's rows: paging, counting and editing.

use tauri::State;

use super::state::AppState;
use crate::db::model::{JsonMap, RowChange, TableDataParams, TableDataResponse};

/// One page of a table; without params, the first 50 rows unsorted and unfiltered.
#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    params: Option<TableDataParams>,
) -> Result<TableDataResponse, String> {
    state
        .driver(&session_id)
        .await?
        .data()
        .table_page(&schema, &table, params.unwrap_or_default())
        .await
}

/// Exact number of rows of a table, for when the view only shows an estimate.
#[tauri::command]
pub async fn count_table_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<i64, String> {
    state.driver(&session_id).await?.data().count_rows(&schema, &table).await
}

/// Inserts one row. Only the columns present in `values` are written; the rest get
/// their defaults. A null value is SQL NULL.
#[tauri::command]
pub async fn insert_row(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    values: JsonMap,
) -> Result<u64, String> {
    state.driver(&session_id).await?.data().insert_row(&schema, &table, values).await
}

/// Saves edited rows all at once: either every row is saved or none is.
#[tauri::command]
pub async fn update_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    changes: Vec<RowChange>,
) -> Result<u64, String> {
    state.driver(&session_id).await?.data().save_rows(&schema, &table, changes).await
}

#[tauri::command]
pub async fn delete_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    primary_keys: Vec<JsonMap>,
) -> Result<serde_json::Value, String> {
    let deleted = state.driver(&session_id).await?.data().delete_rows(&schema, &table, primary_keys).await?;
    Ok(serde_json::json!({ "rowsDeleted": deleted }))
}

/// Inserts imported rows (e.g. from a CSV file) all at once: a bad row leaves the table
/// untouched.
#[tauri::command]
pub async fn import_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<u64, String> {
    state.driver(&session_id).await?.data().import_rows(&schema, &table, columns, rows).await
}
