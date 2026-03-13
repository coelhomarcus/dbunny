use serde::Serialize;
use tauri::State;

use super::state::AppState;

#[derive(Serialize)]
pub struct SchemaInfo {
    pub name: String,
}

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    #[serde(rename = "type")]
    pub table_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub ordinal_position: i32,
    pub is_primary_key: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub schema: String,
    pub return_type: String,
    pub argument_types: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDetail {
    pub name: String,
    pub schema: String,
    pub return_type: String,
    pub argument_types: String,
    pub language: String,
    pub source: String,
    pub kind: String,
    pub volatility: String,
    pub is_strict: bool,
    pub owner: String,
    pub description: Option<String>,
}

#[tauri::command]
pub async fn get_schemas(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<SchemaInfo>, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or("Session not found")?;

    let rows = session
        .client
        .query(
            "SELECT schema_name as name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast') \
             ORDER BY schema_name",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(|r| SchemaInfo { name: r.get("name") }).collect())
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let rows = session
        .client
        .query(
            "SELECT table_name as name, table_schema as schema, \
             CASE WHEN table_type = 'BASE TABLE' THEN 'table' ELSE 'view' END as type \
             FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| TableInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            table_type: r.get("type"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_views(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let rows = session
        .client
        .query(
            "SELECT table_name as name, table_schema as schema, 'view' as type \
             FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'VIEW' \
             ORDER BY table_name",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| TableInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            table_type: r.get("type"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_functions(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<FunctionInfo>, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let rows = session
        .client
        .query(
            "SELECT p.proname as name, n.nspname as schema, \
             pg_get_function_result(p.oid) as return_type, \
             pg_get_function_arguments(p.oid) as argument_types \
             FROM pg_proc p \
             JOIN pg_namespace n ON p.pronamespace = n.oid \
             WHERE n.nspname = $1 AND p.prokind IN ('f', 'p') \
             ORDER BY p.proname",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| FunctionInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            return_type: r.get("return_type"),
            argument_types: r.get("argument_types"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_function_detail(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    function_name: String,
    argument_types: String,
) -> Result<FunctionDetail, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let row = session
        .client
        .query_one(
            "SELECT p.proname as name, \
             n.nspname as schema, \
             pg_get_function_result(p.oid) as return_type, \
             pg_get_function_arguments(p.oid) as argument_types, \
             l.lanname as language, \
             pg_get_functiondef(p.oid) as source, \
             CASE p.prokind \
               WHEN 'f' THEN 'function' \
               WHEN 'p' THEN 'procedure' \
               WHEN 'a' THEN 'aggregate' \
               WHEN 'w' THEN 'window' \
             END as kind, \
             CASE p.provolatile \
               WHEN 'i' THEN 'IMMUTABLE' \
               WHEN 's' THEN 'STABLE' \
               WHEN 'v' THEN 'VOLATILE' \
             END as volatility, \
             p.proisstrict as is_strict, \
             pg_catalog.pg_get_userbyid(p.proowner) as owner, \
             d.description \
             FROM pg_proc p \
             JOIN pg_namespace n ON p.pronamespace = n.oid \
             JOIN pg_language l ON p.prolang = l.oid \
             LEFT JOIN pg_description d ON d.objoid = p.oid AND d.classoid = 'pg_proc'::regclass \
             WHERE n.nspname = $1 AND p.proname = $2 \
               AND pg_get_function_arguments(p.oid) = $3 \
             LIMIT 1",
            &[&schema, &function_name, &argument_types],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(FunctionDetail {
        name: row.get("name"),
        schema: row.get("schema"),
        return_type: row.get("return_type"),
        argument_types: row.get("argument_types"),
        language: row.get("language"),
        source: row.get::<_, Option<String>>("source").unwrap_or_default(),
        kind: row.get("kind"),
        volatility: row.get("volatility"),
        is_strict: row.get("is_strict"),
        owner: row.get("owner"),
        description: row.get("description"),
    })
}

#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<Vec<ColumnInfo>, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let rows = session
        .client
        .query(
            "SELECT c.column_name as name, c.data_type as data_type, \
             (c.is_nullable = 'YES') as is_nullable, \
             c.column_default as default_value, \
             c.ordinal_position::int as ordinal_position, \
             EXISTS ( \
               SELECT 1 FROM information_schema.table_constraints tc \
               JOIN information_schema.key_column_usage kcu \
                 ON tc.constraint_name = kcu.constraint_name \
                 AND tc.table_schema = kcu.table_schema \
                 AND tc.table_name = kcu.table_name \
               WHERE tc.constraint_type = 'PRIMARY KEY' \
                 AND tc.table_schema = $1 \
                 AND tc.table_name = $2 \
                 AND kcu.column_name = c.column_name \
             ) as is_primary_key \
             FROM information_schema.columns c \
             WHERE c.table_schema = $1 AND c.table_name = $2 \
             ORDER BY c.ordinal_position",
            &[&schema, &table],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| ColumnInfo {
            name: r.get("name"),
            data_type: r.get("data_type"),
            is_nullable: r.get("is_nullable"),
            default_value: r.get("default_value"),
            ordinal_position: r.get("ordinal_position"),
            is_primary_key: r.get("is_primary_key"),
        })
        .collect())
}
