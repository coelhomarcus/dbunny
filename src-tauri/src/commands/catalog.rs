//! What the database holds: the explorer, columns, structure, autocomplete.

use tauri::State;

use super::state::AppState;
use crate::db::model::{
    ColumnInfo, CompletionSchema, ExplorerTree, ExtensionInfo, FunctionDetail, ObjectDefinition, TableStructure,
};

/// Every schema with its objects, in one round trip.
#[tauri::command]
pub async fn get_explorer(state: State<'_, AppState>, session_id: String) -> Result<ExplorerTree, String> {
    state.driver(&session_id).await?.catalog().explorer().await
}

#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<Vec<ColumnInfo>, String> {
    state.driver(&session_id).await?.catalog().columns(&schema, &table).await
}

/// What a relation is: "table", "partitioned", "foreign", "view" or "matview"; None when
/// there's no relation by that name.
#[tauri::command]
pub async fn get_relation_kind(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
) -> Result<Option<String>, String> {
    state.driver(&session_id).await?.catalog().relation_kind(&schema, &name).await
}

/// Everything the Structure tab shows about a table or view, in one call.
#[tauri::command]
pub async fn get_table_structure(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<TableStructure, String> {
    state.driver(&session_id).await?.catalog().table_structure(&schema, &table).await
}

/// Everything the SQL editor needs for autocomplete.
#[tauri::command]
pub async fn get_completion_schema(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<CompletionSchema, String> {
    state.driver(&session_id).await?.catalog().completion_schema().await
}

#[tauri::command]
pub async fn get_object_definition(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
) -> Result<ObjectDefinition, String> {
    state.driver(&session_id).await?.catalog().object_definition(&schema, &name, &kind).await
}

#[tauri::command]
pub async fn get_function_detail(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    function_name: String,
    argument_types: String,
) -> Result<FunctionDetail, String> {
    state
        .driver(&session_id)
        .await?
        .catalog()
        .function_detail(&schema, &function_name, &argument_types)
        .await
}

#[tauri::command]
pub async fn get_extensions(state: State<'_, AppState>, session_id: String) -> Result<Vec<ExtensionInfo>, String> {
    state.driver(&session_id).await?.catalog().extensions().await
}
