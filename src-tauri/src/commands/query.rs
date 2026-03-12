use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::State;
use tokio_postgres::types::Type;
use tokio_postgres::Row;
use uuid::Uuid;

use super::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
    pub duration: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResponse {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    pub total_rows: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub sort_column: Option<String>,
    pub sort_direction: Option<String>,
}

fn row_to_json_values(row: &Row) -> Vec<JsonValue> {
    let columns = row.columns();
    let mut values = Vec::with_capacity(columns.len());

    for (i, col) in columns.iter().enumerate() {
        let value = match *col.type_() {
            Type::BOOL => row
                .get::<_, Option<bool>>(i)
                .map(JsonValue::Bool)
                .unwrap_or(JsonValue::Null),
            Type::INT2 => row
                .get::<_, Option<i16>>(i)
                .map(|v| JsonValue::Number(v.into()))
                .unwrap_or(JsonValue::Null),
            Type::INT4 => row
                .get::<_, Option<i32>>(i)
                .map(|v| JsonValue::Number(v.into()))
                .unwrap_or(JsonValue::Null),
            Type::INT8 => row
                .get::<_, Option<i64>>(i)
                .map(|v| JsonValue::Number(v.into()))
                .unwrap_or(JsonValue::Null),
            Type::FLOAT4 => row
                .get::<_, Option<f32>>(i)
                .map(|v| {
                    serde_json::Number::from_f64(v as f64)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null)
                })
                .unwrap_or(JsonValue::Null),
            Type::FLOAT8 | Type::NUMERIC => row
                .get::<_, Option<f64>>(i)
                .map(|v| {
                    serde_json::Number::from_f64(v)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null)
                })
                .unwrap_or(JsonValue::Null),
            Type::JSON | Type::JSONB => row
                .get::<_, Option<JsonValue>>(i)
                .unwrap_or(JsonValue::Null),
            Type::TIMESTAMP => row
                .get::<_, Option<NaiveDateTime>>(i)
                .map(|v| JsonValue::String(v.format("%Y-%m-%d %H:%M:%S").to_string()))
                .unwrap_or(JsonValue::Null),
            Type::TIMESTAMPTZ => row
                .get::<_, Option<DateTime<Utc>>>(i)
                .map(|v| JsonValue::String(v.format("%Y-%m-%d %H:%M:%S%z").to_string()))
                .unwrap_or(JsonValue::Null),
            Type::DATE => row
                .get::<_, Option<NaiveDate>>(i)
                .map(|v| JsonValue::String(v.format("%Y-%m-%d").to_string()))
                .unwrap_or(JsonValue::Null),
            Type::TIME => row
                .get::<_, Option<NaiveTime>>(i)
                .map(|v| JsonValue::String(v.format("%H:%M:%S").to_string()))
                .unwrap_or(JsonValue::Null),
            Type::UUID => row
                .get::<_, Option<Uuid>>(i)
                .map(|v| JsonValue::String(v.to_string()))
                .unwrap_or(JsonValue::Null),
            _ => {
                // Fall back to string representation for all other types
                match row.try_get::<_, Option<String>>(i) {
                    Ok(Some(s)) => JsonValue::String(s),
                    Ok(None) => JsonValue::Null,
                    Err(_) => JsonValue::String("<binary>".to_string()),
                }
            }
        };
        values.push(value);
    }

    values
}

fn columns_from_statement(columns: &[tokio_postgres::Column]) -> Vec<QueryColumn> {
    columns
        .iter()
        .map(|c| QueryColumn {
            name: c.name().to_string(),
            data_type: c.type_().oid().to_string(),
        })
        .collect()
}

fn json_value_to_pg_literal(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => "NULL".to_string(),
        JsonValue::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            format!("'{}'", v.to_string().replace('\'', "''"))
        }
    }
}

async fn get_valid_columns(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    let rows = client
        .query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2",
            &[&schema, &table],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
) -> Result<QueryResult, String> {
    if sql.trim().is_empty() {
        return Err("SQL query is required".into());
    }

    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let start = std::time::Instant::now();
    let stmt = session
        .client
        .prepare(&sql)
        .await
        .map_err(|e| e.to_string())?;

    let columns = columns_from_statement(stmt.columns());

    let rows = session
        .client
        .query(&stmt, &[])
        .await
        .map_err(|e| e.to_string())?;

    let duration = start.elapsed().as_millis() as u64;
    let row_count = rows.len();
    let json_rows: Vec<Vec<JsonValue>> = rows.iter().map(row_to_json_values).collect();

    Ok(QueryResult {
        columns,
        rows: json_rows,
        row_count,
        duration,
    })
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    params: Option<TableDataParams>,
) -> Result<TableDataResponse, String> {
    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let page = params.as_ref().and_then(|p| p.page).unwrap_or(1).max(1);
    let page_size = params
        .as_ref()
        .and_then(|p| p.page_size)
        .unwrap_or(50)
        .clamp(1, 100);
    let offset = (page - 1) * page_size;

    // Build ORDER BY clause
    let mut order_clause = String::new();
    if let Some(ref p) = params {
        if let Some(ref sort_col) = p.sort_column {
            let valid_columns = get_valid_columns(&session.client, &schema, &table).await?;
            if valid_columns.contains(sort_col) {
                let dir = match p.sort_direction.as_deref() {
                    Some("desc") => "DESC",
                    _ => "ASC",
                };
                order_clause = format!(" ORDER BY \"{}\" {}", sort_col, dir);
            }
        }
    }

    // Get total count
    let count_sql = format!("SELECT COUNT(*) as total FROM \"{}\".\"{}\"", schema, table);
    let count_row = session
        .client
        .query_one(&count_sql as &str, &[])
        .await
        .map_err(|e| e.to_string())?;
    let total_rows: i64 = count_row.get(0);

    // Get paginated data
    let data_sql = format!(
        "SELECT * FROM \"{}\".\"{}\"{}  LIMIT {} OFFSET {}",
        schema, table, order_clause, page_size, offset
    );
    let stmt = session
        .client
        .prepare(&data_sql)
        .await
        .map_err(|e| e.to_string())?;
    let columns = columns_from_statement(stmt.columns());
    let rows = session
        .client
        .query(&stmt, &[])
        .await
        .map_err(|e| e.to_string())?;
    let json_rows: Vec<Vec<JsonValue>> = rows.iter().map(row_to_json_values).collect();

    Ok(TableDataResponse {
        columns,
        rows: json_rows,
        total_rows,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn update_row(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    primary_key: serde_json::Map<String, JsonValue>,
    updates: serde_json::Map<String, JsonValue>,
) -> Result<serde_json::Value, String> {
    if primary_key.is_empty() {
        return Err("primaryKey is required".into());
    }
    if updates.is_empty() {
        return Err("updates is required".into());
    }

    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let valid_columns = get_valid_columns(&session.client, &schema, &table).await?;

    for col in updates.keys().chain(primary_key.keys()) {
        if !valid_columns.contains(col) {
            return Err(format!("Invalid column: {}", col));
        }
    }

    let mut set_clauses = Vec::new();
    for (col, val) in &updates {
        set_clauses.push(format!("\"{}\" = {}", col, json_value_to_pg_literal(val)));
    }

    let mut where_clauses = Vec::new();
    for (col, val) in &primary_key {
        where_clauses.push(format!("\"{}\" = {}", col, json_value_to_pg_literal(val)));
    }

    let sql = format!(
        "UPDATE \"{}\".\"{}\" SET {} WHERE {}",
        schema,
        table,
        set_clauses.join(", "),
        where_clauses.join(" AND ")
    );

    let result = session
        .client
        .execute(&sql as &str, &[])
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "rowsAffected": result }))
}

#[tauri::command]
pub async fn delete_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    primary_keys: Vec<serde_json::Map<String, JsonValue>>,
) -> Result<serde_json::Value, String> {
    if primary_keys.is_empty() {
        return Err("primaryKeys is required".into());
    }

    let sessions = state.sessions.lock().await;
    let session = sessions.get(&session_id).ok_or("Session not found")?;

    let valid_columns = get_valid_columns(&session.client, &schema, &table).await?;
    if let Some(first) = primary_keys.first() {
        for col in first.keys() {
            if !valid_columns.contains(col) {
                return Err(format!("Invalid column: {}", col));
            }
        }
    }

    let mut total_deleted: u64 = 0;
    for pk in &primary_keys {
        let mut where_clauses = Vec::new();
        for (col, val) in pk {
            where_clauses.push(format!("\"{}\" = {}", col, json_value_to_pg_literal(val)));
        }

        let sql = format!(
            "DELETE FROM \"{}\".\"{}\" WHERE {}",
            schema,
            table,
            where_clauses.join(" AND ")
        );

        let result = session
            .client
            .execute(&sql as &str, &[])
            .await
            .map_err(|e| e.to_string())?;

        total_deleted += result;
    }

    Ok(serde_json::json!({ "rowsDeleted": total_deleted }))
}
