use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::State;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{Row, SimpleQueryMessage};
use uuid::Uuid;

use super::pgtext::{is_native, text_to_json};
use super::sqlsplit::split_statements;
use super::state::AppState;
use super::tablefilter::{build_where, quote_ident, TableFilter, TextParam};

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
    pub rows_affected: Option<u64>,
    pub duration: u64,
    /// How many statements the script had (1 for a plain query).
    pub statement_count: usize,
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
    pub filters: Option<Vec<TableFilter>>,
    /// "any" joins the filters with OR; anything else (or none) with AND.
    pub filter_mode: Option<String>,
}

fn try_get_string(row: &Row, i: usize) -> JsonValue {
    match row.try_get::<_, Option<String>>(i) {
        Ok(Some(s)) => JsonValue::String(s),
        Ok(None) => JsonValue::Null,
        Err(_) => JsonValue::String("<binary>".to_string()),
    }
}

fn row_to_json_values(row: &Row) -> Vec<JsonValue> {
    let columns = row.columns();
    let mut values = Vec::with_capacity(columns.len());

    for (i, col) in columns.iter().enumerate() {
        let value = match *col.type_() {
            // --- Boolean ---
            Type::BOOL => match row.try_get::<_, Option<bool>>(i) {
                Ok(Some(v)) => JsonValue::Bool(v),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Integers ---
            Type::INT2 => match row.try_get::<_, Option<i16>>(i) {
                Ok(Some(v)) => JsonValue::Number(v.into()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::INT4 | Type::OID => match row.try_get::<_, Option<i32>>(i) {
                Ok(Some(v)) => JsonValue::Number(v.into()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::INT8 => match row.try_get::<_, Option<i64>>(i) {
                Ok(Some(v)) => JsonValue::Number(v.into()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Floats ---
            Type::FLOAT4 => match row.try_get::<_, Option<f32>>(i) {
                Ok(Some(v)) => serde_json::Number::from_f64(v as f64)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::FLOAT8 => match row.try_get::<_, Option<f64>>(i) {
                Ok(Some(v)) => serde_json::Number::from_f64(v)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Arbitrary precision decimal (NUMERIC / DECIMAL) ---
            // Serialized as string to preserve full precision
            Type::NUMERIC => match row.try_get::<_, Option<Decimal>>(i) {
                Ok(Some(v)) => JsonValue::String(v.to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- JSON / JSONB ---
            Type::JSON | Type::JSONB => match row.try_get::<_, Option<JsonValue>>(i) {
                Ok(Some(v)) => v,
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Date / Time ---
            Type::DATE => match row.try_get::<_, Option<NaiveDate>>(i) {
                Ok(Some(v)) => JsonValue::String(v.format("%Y-%m-%d").to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TIME => match row.try_get::<_, Option<NaiveTime>>(i) {
                Ok(Some(v)) => JsonValue::String(v.format("%H:%M:%S%.f").to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TIMESTAMP => match row.try_get::<_, Option<NaiveDateTime>>(i) {
                Ok(Some(v)) => JsonValue::String(v.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TIMESTAMPTZ => match row.try_get::<_, Option<DateTime<Utc>>>(i) {
                Ok(Some(v)) => JsonValue::String(v.format("%Y-%m-%d %H:%M:%S%.f%z").to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- UUID ---
            Type::UUID => match row.try_get::<_, Option<Uuid>>(i) {
                Ok(Some(v)) => JsonValue::String(v.to_string()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Binary ---
            Type::BYTEA => match row.try_get::<_, Option<Vec<u8>>>(i) {
                Ok(Some(v)) => JsonValue::String(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &v,
                )),
                Ok(None) => JsonValue::Null,
                Err(_) => JsonValue::String("<binary>".to_string()),
            },

            // --- Arrays ---
            Type::BOOL_ARRAY => match row.try_get::<_, Option<Vec<bool>>>(i) {
                Ok(Some(v)) => JsonValue::Array(v.into_iter().map(JsonValue::Bool).collect()),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::INT2_ARRAY => match row.try_get::<_, Option<Vec<i16>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| JsonValue::Number(n.into()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::INT4_ARRAY | Type::OID_ARRAY => match row.try_get::<_, Option<Vec<i32>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| JsonValue::Number(n.into()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::INT8_ARRAY => match row.try_get::<_, Option<Vec<i64>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| JsonValue::Number(n.into()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::FLOAT4_ARRAY => match row.try_get::<_, Option<Vec<f32>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| {
                            serde_json::Number::from_f64(n as f64)
                                .map(JsonValue::Number)
                                .unwrap_or(JsonValue::Null)
                        })
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::FLOAT8_ARRAY => match row.try_get::<_, Option<Vec<f64>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| {
                            serde_json::Number::from_f64(n)
                                .map(JsonValue::Number)
                                .unwrap_or(JsonValue::Null)
                        })
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::NUMERIC_ARRAY => match row.try_get::<_, Option<Vec<Decimal>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|n| JsonValue::String(n.to_string()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TEXT_ARRAY | Type::VARCHAR_ARRAY | Type::BPCHAR_ARRAY | Type::NAME_ARRAY => {
                match row.try_get::<_, Option<Vec<String>>>(i) {
                    Ok(Some(v)) => {
                        JsonValue::Array(v.into_iter().map(JsonValue::String).collect())
                    }
                    Ok(None) => JsonValue::Null,
                    Err(_) => try_get_string(row, i),
                }
            }
            Type::UUID_ARRAY => match row.try_get::<_, Option<Vec<Uuid>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|u| JsonValue::String(u.to_string()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::DATE_ARRAY => match row.try_get::<_, Option<Vec<NaiveDate>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|d| JsonValue::String(d.format("%Y-%m-%d").to_string()))
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TIMESTAMP_ARRAY => match row.try_get::<_, Option<Vec<NaiveDateTime>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|d| {
                            JsonValue::String(d.format("%Y-%m-%d %H:%M:%S%.f").to_string())
                        })
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },
            Type::TIMESTAMPTZ_ARRAY => match row.try_get::<_, Option<Vec<DateTime<Utc>>>>(i) {
                Ok(Some(v)) => JsonValue::Array(
                    v.into_iter()
                        .map(|d| {
                            JsonValue::String(d.format("%Y-%m-%d %H:%M:%S%.f%z").to_string())
                        })
                        .collect(),
                ),
                Ok(None) => JsonValue::Null,
                Err(_) => try_get_string(row, i),
            },

            // --- Fallback: text, char, varchar, money, interval, geometric, network,
            //     range, tsvector, tsquery, xml, bit, oid subtypes, etc. ---
            _ => try_get_string(row, i),
        };
        values.push(value);
    }

    values
}

/// Error as JSON for the UI. `offset` shifts the position to be relative to the whole
/// script, and `statement` is the (1-based index, total) of the failing statement
/// when the script had more than one.
fn pg_error_in_script(e: tokio_postgres::Error, offset: usize, statement: Option<(usize, usize)>) -> String {
    if let Some(db) = e.as_db_error() {
        let position = match db.position() {
            Some(tokio_postgres::error::ErrorPosition::Original(p)) => Some(*p as usize + offset),
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

fn columns_from_statement(columns: &[tokio_postgres::Column]) -> Vec<QueryColumn> {
    columns
        .iter()
        .map(|c| QueryColumn {
            name: c.name().to_string(),
            data_type: c.type_().name().to_string(),
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

/// Result of running one statement.
struct StatementOutput {
    columns: Vec<QueryColumn>,
    rows: Vec<Vec<JsonValue>>,
    /// Set for statements that return no rows (INSERT without RETURNING, DDL...).
    rows_affected: Option<u64>,
}

/// Runs a single statement and returns its rows as JSON.
///
/// Types the binary decoders know are read directly. If any column has another type
/// (enum, interval, inet, money, range, domain...), the statement runs through the
/// text protocol and each value is converted from its text form.
async fn run_statement(
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

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
) -> Result<QueryResult, String> {
    let statements = split_statements(&sql);
    if statements.is_empty() {
        return Err("SQL query is required".into());
    }

    let client = state.client(&session_id).await?;
    let total = statements.len();

    let start = std::time::Instant::now();
    // The result set of the last statement that returns rows is the one shown;
    // statements without rows only add to the affected count.
    let mut shown: Option<StatementOutput> = None;
    let mut affected: u64 = 0;
    for (index, statement) in statements.iter().enumerate() {
        let output = run_statement(&client, &statement.text).await.map_err(|e| {
            pg_error_in_script(e, statement.offset, (total > 1).then_some((index + 1, total)))
        })?;
        match output.rows_affected {
            Some(count) => affected += count,
            None => shown = Some(output),
        }
    }
    let duration = start.elapsed().as_millis() as u64;

    Ok(match shown {
        Some(output) => QueryResult {
            row_count: output.rows.len(),
            columns: output.columns,
            rows: output.rows,
            rows_affected: None,
            duration,
            statement_count: total,
        },
        None => QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: affected as usize,
            rows_affected: Some(affected),
            duration,
            statement_count: total,
        },
    })
}

/// The server's own message for a failed statement, without the driver's prefix.
fn db_message(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => db.message().to_string(),
        None => e.to_string(),
    }
}

async fn count_rows(
    client: &tokio_postgres::Client,
    relation: &str,
    where_sql: &str,
    params: &[TextParam],
) -> Result<i64, tokio_postgres::Error> {
    let param_refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
    let row = client
        .query_one(&format!("SELECT COUNT(*) FROM {relation}{where_sql}"), &param_refs)
        .await?;
    Ok(row.get(0))
}

/// One page of a table. The columns the binary decoders can't read are selected as text
/// and converted afterwards, since a statement with parameters can't use the text protocol.
#[allow(clippy::too_many_arguments)]
async fn select_table_page(
    client: &tokio_postgres::Client,
    relation: &str,
    where_sql: &str,
    params: &[TextParam],
    order_clause: &str,
    limit: i64,
    offset: i64,
) -> Result<StatementOutput, tokio_postgres::Error> {
    let probe = client.prepare(&format!("SELECT * FROM {relation}")).await?;
    let types: Vec<Type> = probe.columns().iter().map(|c| c.type_().clone()).collect();
    let select_list = probe
        .columns()
        .iter()
        .map(|c| {
            let name = quote_ident(c.name());
            if is_native(c.type_()) {
                name
            } else {
                format!("{name}::text AS {name}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!("SELECT {select_list} FROM {relation}{where_sql}{order_clause} LIMIT {limit} OFFSET {offset}");
    let stmt = client.prepare(&sql).await?;
    let param_refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
    let rows = client.query(&stmt, &param_refs).await?;

    let mut json_rows: Vec<Vec<JsonValue>> = rows.iter().map(row_to_json_values).collect();
    for row in &mut json_rows {
        for (i, value) in row.iter_mut().enumerate() {
            if !is_native(&types[i]) {
                if let JsonValue::String(text) = value {
                    *value = text_to_json(&types[i], text);
                }
            }
        }
    }

    Ok(StatementOutput {
        columns: columns_from_statement(probe.columns()),
        rows: json_rows,
        rows_affected: None,
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
    let client = state.client(&session_id).await?;

    let page = params.as_ref().and_then(|p| p.page).unwrap_or(1).max(1);
    let page_size = params
        .as_ref()
        .and_then(|p| p.page_size)
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = (page - 1) * page_size;

    let valid_columns = get_valid_columns(&client, &schema, &table).await?;

    // Build ORDER BY clause
    let mut order_clause = String::new();
    if let Some(sort_col) = params.as_ref().and_then(|p| p.sort_column.as_ref()) {
        if valid_columns.contains(sort_col) {
            let dir = match params.as_ref().and_then(|p| p.sort_direction.as_deref()) {
                Some("desc") => "DESC",
                _ => "ASC",
            };
            order_clause = format!(" ORDER BY {} {}", quote_ident(sort_col), dir);
        }
    }

    // Build WHERE clause from the filters
    let filters = params.as_ref().and_then(|p| p.filters.as_deref()).unwrap_or(&[]);
    let match_any = params.as_ref().and_then(|p| p.filter_mode.as_deref()) == Some("any");
    let (where_sql, where_params) = build_where(filters, match_any, &valid_columns)?;

    let relation = format!("{}.{}", quote_ident(&schema), quote_ident(&table));

    // Get total count (of the filtered rows)
    let total_rows = count_rows(&client, &relation, &where_sql, &where_params)
        .await
        .map_err(|e| db_message(&e))?;

    // Get paginated data
    let output = select_table_page(&client, &relation, &where_sql, &where_params, &order_clause, page_size, offset)
        .await
        .map_err(|e| db_message(&e))?;

    Ok(TableDataResponse {
        columns: output.columns,
        rows: output.rows,
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

    let client = state.client(&session_id).await?;

    let valid_columns = get_valid_columns(&client, &schema, &table).await?;

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

    let result = client
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

    let client = state.client(&session_id).await?;

    let valid_columns = get_valid_columns(&client, &schema, &table).await?;
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

        let result = client
            .execute(&sql as &str, &[])
            .await
            .map_err(|e| e.to_string())?;

        total_deleted += result;
    }

    Ok(serde_json::json!({ "rowsDeleted": total_deleted }))
}

/// Tests that need a real server. Run them with:
///   DBUNNY_TEST_DATABASE_URL=postgresql://user:pass@localhost/db cargo test --lib -- --ignored
/// Everything they create is temporary (pg_temp) and disappears with the connection.
#[cfg(test)]
mod server_tests {
    use super::*;
    use serde_json::json;

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
        let json: serde_json::Value = serde_json::from_str(&pg_error_in_script(err, statements[1].offset, Some((2, 2)))).unwrap();
        let position = json["position"].as_u64().unwrap() as usize;
        // The position points at "nope", counted in characters from the start of the script
        let at_position: String = script.chars().skip(position - 1).take(4).collect();
        assert_eq!(at_position, "nope");
        assert_eq!(json["statement"], 2);
        assert_eq!(json["statementCount"], 2);
    }

    // --- table filters ---

    async fn filter_fixture(client: &tokio_postgres::Client) {
        client.batch_execute("CREATE TYPE pg_temp.mood AS ENUM ('sad', 'happy')").await.unwrap();
        client
            .batch_execute(
                "CREATE TEMP TABLE filt (id int PRIMARY KEY, name text, mood pg_temp.mood, tags text[]); \
                 INSERT INTO filt VALUES \
                   (1, 'ana', 'happy', ARRAY['a']), \
                   (2, 'bruno', 'sad', NULL), \
                   (3, NULL, 'happy', ARRAY['b','c']), \
                   (4, 'ana maria', NULL, NULL)",
            )
            .await
            .unwrap();
    }

    fn filter(column: &str, operator: &str, value: &str) -> TableFilter {
        TableFilter { column: column.into(), operator: operator.into(), value: value.into() }
    }

    const COLUMNS: [&str; 4] = ["id", "name", "mood", "tags"];

    /// Ids of the rows matching the filters, plus the count the table view would report.
    async fn matching_ids(
        client: &tokio_postgres::Client,
        filters: &[TableFilter],
        match_any: bool,
    ) -> Result<(Vec<i64>, i64), String> {
        let valid: Vec<String> = COLUMNS.iter().map(|c| c.to_string()).collect();
        let (where_sql, params) = build_where(filters, match_any, &valid)?;
        let page = select_table_page(client, "pg_temp.filt", &where_sql, &params, " ORDER BY \"id\"", 100, 0)
            .await
            .map_err(|e| db_message(&e))?;
        let count = count_rows(client, "pg_temp.filt", &where_sql, &params).await.map_err(|e| db_message(&e))?;
        let ids = page.rows.iter().map(|r| r[0].as_i64().unwrap()).collect();
        Ok((ids, count))
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn filters_compare_with_the_column_type() {
        let client = connect().await;
        filter_fixture(&client).await;
        assert_eq!(matching_ids(&client, &[filter("id", ">=", "3")], false).await.unwrap(), (vec![3, 4], 2));
        assert_eq!(matching_ids(&client, &[filter("id", "!=", "1")], false).await.unwrap(), (vec![2, 3, 4], 3));
        assert_eq!(matching_ids(&client, &[filter("id", "in", "1, 4, 9")], false).await.unwrap(), (vec![1, 4], 2));
        assert_eq!(matching_ids(&client, &[filter("id", "not in", "1,2")], false).await.unwrap(), (vec![3, 4], 2));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn text_filters_and_null_checks_work() {
        let client = connect().await;
        filter_fixture(&client).await;
        assert_eq!(matching_ids(&client, &[filter("name", "like", "ana%")], false).await.unwrap().0, vec![1, 4]);
        assert_eq!(matching_ids(&client, &[filter("name", "ilike", "ANA")], false).await.unwrap().0, vec![1]);
        assert_eq!(matching_ids(&client, &[filter("name", "not like", "ana%")], false).await.unwrap().0, vec![2]);
        assert_eq!(matching_ids(&client, &[filter("name", "is null", "")], false).await.unwrap().0, vec![3]);
        assert_eq!(matching_ids(&client, &[filter("tags", "is not null", "")], false).await.unwrap().0, vec![1, 3]);
        // like on a non-text column compares its text form
        assert_eq!(matching_ids(&client, &[filter("id", "like", "1%")], false).await.unwrap().0, vec![1]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn enum_columns_can_be_filtered_and_are_still_readable() {
        let client = connect().await;
        filter_fixture(&client).await;
        let valid: Vec<String> = COLUMNS.iter().map(|c| c.to_string()).collect();
        let (where_sql, params) = build_where(&[filter("mood", "=", "happy")], false, &valid).unwrap();
        let page = select_table_page(&client, "pg_temp.filt", &where_sql, &params, " ORDER BY \"id\"", 100, 0).await.unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0], vec![json!(1), json!("ana"), json!("happy"), json!(["a"])]);
        assert_eq!(page.rows[1], vec![json!(3), JsonValue::Null, json!("happy"), json!(["b", "c"])]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn several_filters_match_all_or_any() {
        let client = connect().await;
        filter_fixture(&client).await;
        let filters = [filter("name", "like", "ana%"), filter("id", ">", "1")];
        assert_eq!(matching_ids(&client, &filters, false).await.unwrap().0, vec![4]);
        assert_eq!(matching_ids(&client, &filters, true).await.unwrap().0, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_value_of_the_wrong_type_fails_with_the_servers_message() {
        let client = connect().await;
        filter_fixture(&client).await;
        let err = matching_ids(&client, &[filter("id", "=", "abc")], false).await.unwrap_err();
        assert!(err.contains("invalid input syntax for type integer"), "{err}");
        assert!(!err.contains("db error"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn filter_values_are_never_run_as_sql() {
        let client = connect().await;
        filter_fixture(&client).await;
        let evil = filter("name", "=", "x'; DROP TABLE filt; --");
        assert_eq!(matching_ids(&client, &[evil], false).await.unwrap(), (vec![], 0));
        // The table is still there
        assert_eq!(matching_ids(&client, &[], false).await.unwrap().1, 4);
    }
}

