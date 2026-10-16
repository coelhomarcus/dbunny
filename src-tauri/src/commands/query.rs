use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use tauri::State;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{Row, SimpleQueryMessage};
use uuid::Uuid;

use super::pgtext::{int_to_json, is_native, text_to_json};
use super::sqlsplit::split_statements;
use super::state::AppState;
use super::tableedit::{build_delete, build_import_batches, build_insert, build_update};
use super::tablefilter::{build_where, quote_ident, TextParam};
use crate::db::model::{QueryColumn, QueryResult, RowChange, SortKey, TableDataParams, TableDataResponse};

/// ` ORDER BY ...` for the sort keys that name real columns (others are ignored).
pub(crate) fn order_clause(keys: &[SortKey], valid_columns: &[String]) -> String {
    let parts: Vec<String> = keys
        .iter()
        .filter(|k| valid_columns.contains(&k.column))
        .map(|k| format!("{} {}", quote_ident(&k.column), if k.direction.as_deref() == Some("desc") { "DESC" } else { "ASC" }))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", parts.join(", "))
    }
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
                Ok(Some(v)) => int_to_json(v),
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
                Ok(Some(v)) => JsonValue::Array(v.into_iter().map(int_to_json).collect()),
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
pub(crate) fn pg_error_in_script(e: tokio_postgres::Error, offset: i64, statement: Option<(usize, usize)>) -> String {
    if let Some(db) = e.as_db_error() {
        let position = match db.position() {
            Some(tokio_postgres::error::ErrorPosition::Original(p)) => Some((*p as i64 + offset).max(1)),
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

async fn get_valid_columns(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    let rows = client
        .query(
            // pg_catalog, so materialized views have columns too
            "SELECT a.attname::text FROM pg_attribute a \
             JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped",
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

/// Rows of a change shown in the editor; the count of every changed row still comes back.
const CHANGED_ROWS_SHOWN: usize = 500;
const CHANGED_PREFIX: &str = "WITH dbunny_changed AS (\n";

/// The first word of a statement, past comments.
fn leading_keyword(sql: &str) -> String {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else {
            break;
        }
    }
    rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase()
}

/// An INSERT, UPDATE or DELETE rewritten to also return the rows it changed: the statement
/// still runs once, the server counts every changed row and sends back the first ones.
/// None for anything else, or when the statement already says what to return (RETURNING)
/// or brings its own WITH, which this can't nest safely.
fn with_changed_rows(sql: &str) -> Option<String> {
    if !matches!(leading_keyword(sql).as_str(), "insert" | "update" | "delete") {
        return None;
    }
    let lower = sql.to_ascii_lowercase();
    let has_word = |word: &str| {
        lower.match_indices(word).any(|(i, _)| {
            let before = lower[..i].chars().next_back();
            let after = lower[i + word.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    };
    if has_word("returning") {
        return None;
    }
    // On new lines, so a comment at the end of the statement can't swallow what follows
    Some(format!(
        "{CHANGED_PREFIX}{sql}\nRETURNING *\n) SELECT (SELECT count(*) FROM dbunny_changed) AS dbunny_total, * FROM dbunny_changed LIMIT {CHANGED_ROWS_SHOWN}"
    ))
}

/// The rewritten statement's output as the changed rows: the count column goes, and the
/// count becomes the number of rows affected.
fn changed_rows_output(mut output: StatementOutput) -> StatementOutput {
    let total = output
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0);
    output.columns.remove(0);
    for row in &mut output.rows {
        row.remove(0);
    }
    output.rows_affected = Some(total);
    output
}

/// Runs one statement of the editor's script, with the changed rows of an INSERT, UPDATE
/// or DELETE. A rewrite the server can't take (an insert into a view whose rules don't
/// return rows, say) is refused before anything runs, and then the statement runs as typed.
async fn run_editor_statement(
    client: &tokio_postgres::Client,
    sql: &str,
    offset: i64,
    in_transaction: bool,
    position: Option<(usize, usize)>,
) -> Result<StatementOutput, String> {
    if let Some(rewritten) = with_changed_rows(sql) {
        match run_statement(client, &rewritten).await {
            Ok(output) => return Ok(changed_rows_output(output)),
            // In a transaction the failure has aborted it, so there's no second try
            Err(e) if !in_transaction && e.code() == Some(&tokio_postgres::error::SqlState::FEATURE_NOT_SUPPORTED) => {}
            Err(e) => return Err(pg_error_in_script(e, offset - CHANGED_PREFIX.chars().count() as i64, position)),
        }
    }
    run_statement(client, sql).await.map_err(|e| pg_error_in_script(e, offset, position))
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

    let (client, in_transaction) = state.postgres(&session_id).await?.editor_client().await?;
    let notices = state.postgres(&session_id).await?.notices().await?;
    // Only this run's notices are returned
    notices.lock().unwrap().clear();
    let total = statements.len();

    let start = std::time::Instant::now();
    // The result set of the last statement that returns rows is the one shown;
    // statements without rows only add to the affected count.
    let mut shown: Option<StatementOutput> = None;
    let mut affected: u64 = 0;
    for (index, statement) in statements.iter().enumerate() {
        let position = (total > 1).then_some((index + 1, total));
        let output = run_editor_statement(&client, &statement.text, statement.offset as i64, in_transaction, position).await?;
        match output.rows_affected {
            // A change that brought its rows back is shown, and still counts as affected
            Some(count) if !output.columns.is_empty() => {
                affected += count;
                shown = Some(output);
            }
            Some(count) => affected += count,
            None => shown = Some(output),
        }
    }
    let duration = start.elapsed().as_millis() as u64;
    let notices = std::mem::take(&mut *notices.lock().unwrap());

    Ok(match shown {
        Some(output) => QueryResult {
            row_count: output.rows.len(),
            columns: output.columns,
            rows: output.rows,
            // The changed rows on show: the count of all of them
            rows_affected: output.rows_affected,
            duration,
            statement_count: total,
            notices,
            in_transaction,
        },
        None => QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: affected as usize,
            rows_affected: Some(affected),
            duration,
            statement_count: total,
            notices,
            in_transaction,
        },
    })
}

/// The server's own message for a failed statement, without the driver's prefix.
pub(crate) fn db_message(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => db.message().to_string(),
        None => e.to_string(),
    }
}

/// Below this many rows an exact count is cheap, and the statistics are often stale.
const ESTIMATE_THRESHOLD: i64 = 100_000;

/// Row estimate from the table statistics (updated by VACUUM / ANALYZE), or None for
/// views, tables never analyzed, and anything small enough to just count.
async fn estimated_rows(
    client: &tokio_postgres::Client,
    relation: &str,
) -> Result<Option<i64>, tokio_postgres::Error> {
    let row = client
        .query_opt(
            "SELECT reltuples::bigint FROM pg_class \
             WHERE oid = to_regclass($1) AND relkind IN ('r', 'p', 'm')",
            &[&relation],
        )
        .await?;
    Ok(row
        .map(|r| r.get::<_, i64>(0))
        .filter(|&estimate| estimate >= ESTIMATE_THRESHOLD))
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
    let client = state.postgres(&session_id).await?.client().await?;

    let page = params.as_ref().and_then(|p| p.page).unwrap_or(1).max(1);
    let page_size = params
        .as_ref()
        .and_then(|p| p.page_size)
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = (page - 1) * page_size;

    let valid_columns = get_valid_columns(&client, &schema, &table).await?;

    let sort_keys = match params.as_ref() {
        Some(TableDataParams { sort: Some(keys), .. }) => keys.clone(),
        Some(TableDataParams { sort_column: Some(column), sort_direction, .. }) => {
            vec![SortKey { column: column.clone(), direction: sort_direction.clone() }]
        }
        _ => vec![],
    };
    let order_clause = order_clause(&sort_keys, &valid_columns);

    // Build WHERE clause from the filters
    let filters = params.as_ref().and_then(|p| p.filters.as_deref()).unwrap_or(&[]);
    let match_any = params.as_ref().and_then(|p| p.filter_mode.as_deref()) == Some("any");
    let (where_sql, where_params) = build_where(filters, match_any, &valid_columns)?;

    let relation = format!("{}.{}", quote_ident(&schema), quote_ident(&table));

    // Total rows: exact for small tables or when filtering, estimated for big unfiltered ones
    let estimate = if where_sql.is_empty() {
        estimated_rows(&client, &relation).await.map_err(|e| db_message(&e))?
    } else {
        None
    };
    let (total_rows, total_rows_estimated) = match estimate {
        Some(estimate) => (estimate, true),
        None => (
            count_rows(&client, &relation, &where_sql, &where_params)
                .await
                .map_err(|e| db_message(&e))?,
            false,
        ),
    };

    // Get paginated data
    let output = select_table_page(&client, &relation, &where_sql, &where_params, &order_clause, page_size, offset)
        .await
        .map_err(|e| db_message(&e))?;

    Ok(TableDataResponse {
        columns: output.columns,
        rows: output.rows,
        total_rows,
        total_rows_estimated,
        page,
        page_size,
    })
}

/// Inserts one row. Only the columns present in `values` are written; the rest get
/// their defaults. A null value is SQL NULL.
#[tauri::command]
pub async fn insert_row(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    values: serde_json::Map<String, JsonValue>,
) -> Result<u64, String> {
    let client = state.postgres(&session_id).await?.client().await?;
    let valid_columns = get_valid_columns(&client, &schema, &table).await?;
    let relation = format!("{}.{}", quote_ident(&schema), quote_ident(&table));
    insert_into(&client, &relation, &values, &valid_columns).await
}

async fn insert_into(
    client: &tokio_postgres::Client,
    relation: &str,
    values: &serde_json::Map<String, JsonValue>,
    valid_columns: &[String],
) -> Result<u64, String> {
    let (sql, params) = build_insert(relation, values, valid_columns)?;
    let param_refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
    client
        .execute(&sql, &param_refs)
        .await
        .map_err(|e| db_message(&e))
}

/// Exact number of rows of a table, for when the view only shows an estimate.
#[tauri::command]
pub async fn count_table_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<i64, String> {
    let client = state.postgres(&session_id).await?.client().await?;
    let relation = format!("{}.{}", quote_ident(&schema), quote_ident(&table));
    count_rows(&client, &relation, "", &[])
        .await
        .map_err(|e| db_message(&e))
}

/// Saves edited rows all at once: a separate connection runs them in one transaction, so
/// either every row is saved or none is.
#[tauri::command]
pub async fn update_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    changes: Vec<RowChange>,
) -> Result<u64, String> {
    let link = state.postgres(&session_id).await?.link().await?;
    let notices = state.postgres(&session_id).await?.notices().await?;
    let client = crate::db::postgres::connect::open_connection(&link, &notices)
        .await
        .map_err(|e| format!("Couldn't open a connection to save: {e}"))?;
    save_rows(&client, &schema, &table, &changes).await
}

/// Each change must find exactly its row: one that was changed or deleted elsewhere since
/// it was read (or a key that matches several rows) stops the save instead of passing silently.
pub(crate) async fn save_rows(client: &tokio_postgres::Client, schema: &str, table: &str, changes: &[RowChange]) -> Result<u64, String> {
    client.batch_execute("BEGIN").await.map_err(|e| db_message(&e))?;
    let total = changes.len();
    for (i, change) in changes.iter().enumerate() {
        let failure = match update_in(client, schema, table, &change.primary_key, &change.updates).await {
            Ok(1) => continue,
            Ok(0) => "it wasn't found, so it was changed or deleted since it was loaded".to_string(),
            Ok(n) => format!("its key matched {n} rows"),
            Err(e) => e,
        };
        let _ = client.batch_execute("ROLLBACK").await;
        let which = if total > 1 { format!("Row {} of {total}", i + 1) } else { "The row".to_string() };
        return Err(format!("Nothing was saved. {which}: {failure}"));
    }
    client.batch_execute("COMMIT").await.map_err(|e| format!("Nothing was saved: {}", db_message(&e)))?;
    Ok(total as u64)
}

async fn update_in(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
    primary_key: &serde_json::Map<String, JsonValue>,
    updates: &serde_json::Map<String, JsonValue>,
) -> Result<u64, String> {
    let valid_columns = get_valid_columns(client, schema, table).await?;
    let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let (sql, params) = build_update(&relation, primary_key, updates, &valid_columns)?;
    let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
    client.execute(sql.as_str(), &refs).await.map_err(|e| db_message(&e))
}

#[tauri::command]
pub async fn delete_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    primary_keys: Vec<serde_json::Map<String, JsonValue>>,
) -> Result<serde_json::Value, String> {
    let client = state.postgres(&session_id).await?.client().await?;
    let deleted = delete_from(&client, &schema, &table, &primary_keys).await?;
    Ok(serde_json::json!({ "rowsDeleted": deleted }))
}

async fn delete_from(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
    primary_keys: &[serde_json::Map<String, JsonValue>],
) -> Result<u64, String> {
    let valid_columns = get_valid_columns(client, schema, table).await?;
    let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let (sql, params) = build_delete(&relation, primary_keys, &valid_columns)?;
    let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
    client.execute(sql.as_str(), &refs).await.map_err(|e| db_message(&e))
}

/// Inserts imported rows (e.g. from a CSV file) all at once: a separate connection runs
/// them in one transaction, so a bad row leaves the table untouched.
#[tauri::command]
pub async fn import_rows(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<u64, String> {
    let link = state.postgres(&session_id).await?.link().await?;
    let notices = state.postgres(&session_id).await?.notices().await?;
    let client = crate::db::postgres::connect::open_connection(&link, &notices)
        .await
        .map_err(|e| format!("Couldn't open a connection for the import: {e}"))?;
    import_into(&client, &schema, &table, &columns, &rows).await
}

pub(crate) async fn import_into(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
) -> Result<u64, String> {
    let valid_columns = get_valid_columns(client, schema, table).await?;
    let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let batches = build_import_batches(&relation, columns, rows, &valid_columns)?;

    client.batch_execute("BEGIN").await.map_err(|e| db_message(&e))?;
    let mut inserted = 0;
    for (first_row, sql, params) in &batches {
        let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
        match client.execute(sql.as_str(), &refs).await {
            Ok(count) => inserted += count,
            Err(e) => {
                let _ = client.batch_execute("ROLLBACK").await;
                let size = params.len() / columns.len();
                return Err(format!(
                    "Nothing was imported. A row between {} and {} failed: {}",
                    first_row + 1,
                    first_row + size,
                    db_message(&e)
                ));
            }
        }
    }
    client.batch_execute("COMMIT").await.map_err(|e| db_message(&e))?;
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(column: &str, direction: &str) -> SortKey {
        SortKey { column: column.into(), direction: Some(direction.into()) }
    }

    #[test]
    fn only_plain_changes_get_their_rows_back() {
        assert!(with_changed_rows("UPDATE t SET a = 1 WHERE id = 7").is_some());
        assert!(with_changed_rows("  -- fix it\n/* note */ delete from t").is_some());
        assert!(with_changed_rows("insert into t values (1)").is_some());
        assert!(with_changed_rows("SELECT 1").is_none());
        assert!(with_changed_rows("WITH x AS (SELECT 1) UPDATE t SET a = 1").is_none(), "its own WITH");
        assert!(with_changed_rows("UPDATE t SET a = 1 RETURNING id").is_none(), "already returns rows");
        assert!(with_changed_rows("update t set returning_count = 1").is_some(), "a column named like the keyword");
        let sql = with_changed_rows("UPDATE t SET a = 1 -- trailing comment").unwrap();
        assert!(sql.contains("-- trailing comment\nRETURNING *\n)"), "{sql}");
    }

    #[test]
    fn several_sort_keys_keep_their_order_and_skip_unknown_columns() {
        let valid: Vec<String> = ["name", "age", "we\"ird"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            order_clause(&[key("age", "desc"), key("nope; DROP", "asc"), key("name", "asc")], &valid),
            " ORDER BY \"age\" DESC, \"name\" ASC"
        );
        assert_eq!(order_clause(&[key("we\"ird", "sideways")], &valid), " ORDER BY \"we\"\"ird\" ASC");
        assert_eq!(order_clause(&[key("nope", "asc")], &valid), "");
        assert_eq!(order_clause(&[], &valid), "");
    }
}

/// Tests that need a real server. Run them with:
///   DBUNNY_TEST_DATABASE_URL=postgresql://user:pass@localhost/db cargo test --lib -- --ignored
/// Everything they create is temporary (pg_temp) and disappears with the connection.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::commands::tablefilter::TableFilter;
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
        let json: serde_json::Value = serde_json::from_str(&pg_error_in_script(err, statements[1].offset as i64, Some((2, 2)))).unwrap();
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

    // --- row count estimate ---

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn big_analyzed_tables_get_an_estimate_and_small_ones_do_not() {
        let client = connect().await;
        client
            .batch_execute(
                "CREATE TEMP TABLE est_big AS SELECT g AS id FROM generate_series(1, 150000) g; \
                 CREATE TEMP TABLE est_small AS SELECT g AS id FROM generate_series(1, 50) g; \
                 CREATE TEMP VIEW est_view AS SELECT * FROM est_big; \
                 ANALYZE est_big; ANALYZE est_small",
            )
            .await
            .unwrap();

        let estimate = estimated_rows(&client, "pg_temp.\"est_big\"").await.unwrap().expect("a big table is estimated");
        assert!((140_000..=160_000).contains(&estimate), "{estimate}");
        assert_eq!(count_rows(&client, "pg_temp.\"est_big\"", "", &[]).await.unwrap(), 150_000);

        // Small tables, views and unknown names are counted exactly instead
        assert_eq!(estimated_rows(&client, "pg_temp.\"est_small\"").await.unwrap(), None);
        assert_eq!(estimated_rows(&client, "pg_temp.\"est_view\"").await.unwrap(), None);
        assert_eq!(estimated_rows(&client, "pg_temp.\"missing\"").await.unwrap(), None);
    }

    // --- inserting rows ---

    async fn insert_fixture(client: &tokio_postgres::Client) -> Vec<String> {
        client.batch_execute("CREATE TYPE pg_temp.mood AS ENUM ('sad', 'happy')").await.unwrap();
        client
            .batch_execute(
                "CREATE TEMP TABLE ins (id serial PRIMARY KEY, name text NOT NULL, mood pg_temp.mood, \
                 created timestamptz DEFAULT now(), note text DEFAULT 'x')",
            )
            .await
            .unwrap();
        ["id", "name", "mood", "created", "note"].iter().map(|c| c.to_string()).collect()
    }

    fn values(pairs: serde_json::Value) -> serde_json::Map<String, JsonValue> {
        pairs.as_object().unwrap().clone()
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn untouched_columns_get_their_defaults() {
        let client = connect().await;
        let columns = insert_fixture(&client).await;
        let affected = insert_into(&client, "pg_temp.ins", &values(json!({"name": "ana"})), &columns).await.unwrap();
        assert_eq!(affected, 1);
        let row = run(&client, "SELECT id, name, mood, created IS NOT NULL AS has_created, note FROM ins").await;
        assert_eq!(row.rows[0], vec![json!(1), json!("ana"), JsonValue::Null, json!(true), json!("x")]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn an_explicit_null_overrides_the_default() {
        let client = connect().await;
        let columns = insert_fixture(&client).await;
        insert_into(&client, "pg_temp.ins", &values(json!({"name": "ana", "note": null})), &columns).await.unwrap();
        let row = run(&client, "SELECT note FROM ins").await;
        assert_eq!(row.rows[0], vec![JsonValue::Null]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn enums_numbers_and_empty_strings_are_converted_by_the_server() {
        let client = connect().await;
        let columns = insert_fixture(&client).await;
        insert_into(&client, "pg_temp.ins", &values(json!({"id": 42, "name": "", "mood": "happy"})), &columns).await.unwrap();
        let row = run(&client, "SELECT id, name, mood FROM ins").await;
        assert_eq!(row.rows[0], vec![json!(42), json!(""), json!("happy")]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn constraint_and_type_errors_come_back_as_the_servers_message() {
        let client = connect().await;
        let columns = insert_fixture(&client).await;
        // name is NOT NULL and has no default
        let err = insert_into(&client, "pg_temp.ins", &values(json!({})), &columns).await.unwrap_err();
        assert!(err.contains("null value in column \"name\""), "{err}");
        let err = insert_into(&client, "pg_temp.ins", &values(json!({"name": "a", "id": "abc"})), &columns).await.unwrap_err();
        assert!(err.contains("invalid input syntax for type integer"), "{err}");
        let err = insert_into(&client, "pg_temp.ins", &values(json!({"name": "a", "mood": "angry"})), &columns).await.unwrap_err();
        assert!(err.contains("invalid input value for enum"), "{err}");
        // Nothing was inserted by the failures
        assert_eq!(count_rows(&client, "pg_temp.ins", "", &[]).await.unwrap(), 0);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn a_table_of_only_defaults_accepts_an_empty_insert() {
        let client = connect().await;
        client.batch_execute("CREATE TEMP TABLE only_defaults (id serial, tag text DEFAULT 'new')").await.unwrap();
        let columns: Vec<String> = vec!["id".into(), "tag".into()];
        insert_into(&client, "pg_temp.only_defaults", &values(json!({})), &columns).await.unwrap();
        let row = run(&client, "SELECT id, tag FROM only_defaults").await;
        assert_eq!(row.rows[0], vec![json!(1), json!("new")]);
    }

    async fn edit_fixture(client: &tokio_postgres::Client) {
        client
            .batch_execute(
                "CREATE TYPE pg_temp.level AS ENUM ('low', 'high'); \
                 CREATE TEMP TABLE parents (id int PRIMARY KEY); \
                 CREATE TEMP TABLE items ( \
                   id int, at timestamptz, name text, level pg_temp.level, data jsonb, \
                   parent_id int REFERENCES parents (id), \
                   PRIMARY KEY (id, at)); \
                 INSERT INTO parents VALUES (1); \
                 INSERT INTO items VALUES \
                   (1, '2026-01-01T10:00:00Z', 'one', 'low', NULL, NULL), \
                   (2, '2026-01-02T10:00:00Z', 'two', 'low', NULL, NULL), \
                   (3, '2026-01-03T10:00:00Z', 'three', 'low', NULL, NULL);",
            )
            .await
            .unwrap();
    }

    async fn temp_schema(client: &tokio_postgres::Client) -> String {
        client
            .query_one("SELECT nspname::text FROM pg_namespace WHERE oid = pg_my_temp_schema()", &[])
            .await
            .unwrap()
            .get(0)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn updates_convert_values_server_side_and_find_rows_by_composite_keys() {
        let client = connect().await;
        edit_fixture(&client).await;
        let schema = temp_schema(&client).await;

        let key = values(json!({"id": 2, "at": "2026-01-02T10:00:00Z"}));
        let updates = values(json!({"name": "it's \\ fine'); DROP TABLE items; --", "level": "high", "data": "{\"a\": [1, 2]}"}));
        assert_eq!(update_in(&client, &schema, "items", &key, &updates).await.unwrap(), 1);

        let row = client
            .query_one("SELECT name, level::text, data->'a'->>1 FROM items WHERE id = 2", &[])
            .await
            .unwrap();
        assert_eq!(row.get::<_, String>(0), "it's \\ fine'); DROP TABLE items; --");
        assert_eq!(row.get::<_, String>(1), "high");
        assert_eq!(row.get::<_, String>(2), "2");

        // NULL is a real NULL; a key that matches nothing changes nothing
        assert_eq!(update_in(&client, &schema, "items", &key, &values(json!({"name": null}))).await.unwrap(), 1);
        let name: Option<String> = client.query_one("SELECT name FROM items WHERE id = 2", &[]).await.unwrap().get(0);
        assert_eq!(name, None);
        let missing = values(json!({"id": 2, "at": "2030-01-01T00:00:00Z"}));
        assert_eq!(update_in(&client, &schema, "items", &missing, &values(json!({"name": "x"}))).await.unwrap(), 0);

        let err = update_in(&client, &schema, "items", &key, &values(json!({"level": "medium"}))).await.unwrap_err();
        assert!(err.contains("invalid input value for enum"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn big_bigint_keys_keep_every_digit_through_read_edit_and_delete() {
        let client = connect().await;
        client
            .batch_execute(
                "CREATE TEMP TABLE snow (id bigint PRIMARY KEY, name text); \
                 INSERT INTO snow VALUES (1234567890123456789, 'a'), (1234567890123456800, 'b'), (7, 'small')",
            )
            .await
            .unwrap();
        let schema = temp_schema(&client).await;
        let relation = format!("{}.{}", quote_ident(&schema), quote_ident("snow"));
        let page = select_table_page(&client, &relation, "", &[], " ORDER BY id", 50, 0).await.unwrap();
        let ids: Vec<JsonValue> = page.rows.iter().map(|r| r[0].clone()).collect();
        // Small ids stay numbers; the big ones come as text, distinct even where a float would merge them
        assert_eq!(ids, vec![json!(7), json!("1234567890123456789"), json!("1234567890123456800")]);

        // The value as the app holds it finds exactly its row
        let key = values(json!({"id": ids[1]}));
        assert_eq!(update_in(&client, &schema, "snow", &key, &values(json!({"name": "edited"}))).await.unwrap(), 1);
        let names: Vec<String> = client.query("SELECT name FROM snow ORDER BY id", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
        assert_eq!(names, vec!["small", "edited", "b"]);
        assert_eq!(delete_from(&client, &schema, "snow", &[key]).await.unwrap(), 1);
        let left: i64 = client.query_one("SELECT count(*) FROM snow WHERE id = 1234567890123456800", &[]).await.unwrap().get(0);
        assert_eq!(left, 1, "the neighbour a rounded key would have hit is still there");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn saving_edited_rows_is_all_or_nothing_and_needs_each_row() {
        let client = connect().await;
        client.batch_execute("CREATE TEMP TABLE edits (id int PRIMARY KEY, qty int CHECK (qty >= 0)); INSERT INTO edits VALUES (1, 1), (2, 2), (3, 3)").await.unwrap();
        let schema = temp_schema(&client).await;
        let change = |id: i32, qty: i32| RowChange { primary_key: values(json!({"id": id})), updates: values(json!({"qty": qty})) };
        async fn qty(client: &tokio_postgres::Client) -> Vec<i32> {
            client.query("SELECT qty FROM edits ORDER BY id", &[]).await.unwrap().iter().map(|r| r.get(0)).collect()
        }

        // The second row breaks a constraint: the first isn't saved either
        let err = save_rows(&client, &schema, "edits", &[change(1, 10), change(2, -1), change(3, 30)]).await.unwrap_err();
        assert!(err.starts_with("Nothing was saved. Row 2 of 3") && err.contains("check constraint"), "{err}");
        assert_eq!(qty(&client).await, vec![1, 2, 3]);

        // A row deleted elsewhere since it was loaded stops the save
        client.batch_execute("DELETE FROM edits WHERE id = 3").await.unwrap();
        let err = save_rows(&client, &schema, "edits", &[change(1, 10), change(3, 30)]).await.unwrap_err();
        assert!(err.contains("Row 2 of 2") && err.contains("wasn't found"), "{err}");
        assert_eq!(qty(&client).await, vec![1, 2]);

        assert_eq!(save_rows(&client, &schema, "edits", &[change(1, 10), change(2, 20)]).await.unwrap(), 2);
        assert_eq!(qty(&client).await, vec![10, 20]);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn changes_in_the_editor_show_the_rows_they_changed() {
        let client = connect().await;
        client.batch_execute("CREATE TEMP TABLE stock (id int PRIMARY KEY, qty int); INSERT INTO stock SELECT g, 0 FROM generate_series(1, 600) g").await.unwrap();
        let run = |sql: &'static str| {
            let client = &client;
            async move { run_editor_statement(client, sql, 0, false, None).await }
        };

        let out = run("UPDATE stock SET qty = 9 WHERE id = 7 -- one row").await.unwrap();
        assert_eq!(out.rows_affected, Some(1));
        assert_eq!(out.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["id", "qty"]);
        assert_eq!(out.rows, vec![vec![json!(7), json!(9)]]);

        // Every row is changed and counted; the first ones come back
        let out = run("DELETE FROM stock WHERE id > 50").await.unwrap();
        assert_eq!(out.rows_affected, Some(550));
        assert_eq!(out.rows.len(), CHANGED_ROWS_SHOWN.min(550));
        let left: i64 = client.query_one("SELECT count(*) FROM stock", &[]).await.unwrap().get(0);
        assert_eq!(left, 50, "the statement ran once, in full");

        // Nothing matched: no rows, zero affected
        let out = run("UPDATE stock SET qty = 1 WHERE id = -1").await.unwrap();
        assert_eq!((out.rows_affected, out.rows.len()), (Some(0), 0));

        // RETURNING of its own is left as typed
        let out = run("INSERT INTO stock VALUES (1000, 5) RETURNING id").await.unwrap();
        assert_eq!((out.rows_affected, out.rows.clone()), (None, vec![vec![json!(1000)]]));

        // Errors still point into the statement as typed
        let err = run("UPDATE stock SET nope = 1").await.err().expect("an unknown column fails");
        let error: serde_json::Value = serde_json::from_str(&err).unwrap();
        assert_eq!(error["position"], json!(18), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn deletes_are_all_or_nothing() {
        let client = connect().await;
        edit_fixture(&client).await;
        let schema = temp_schema(&client).await;
        let key = |id: i32, day: i32| values(json!({"id": id, "at": format!("2026-01-0{day}T10:00:00Z")}));

        // Row 3 is referenced, so deleting 1 and 3 together must fail and keep both
        client.batch_execute("CREATE TEMP TABLE refs (item_id int, item_at timestamptz, FOREIGN KEY (item_id, item_at) REFERENCES items (id, at)); INSERT INTO refs VALUES (3, '2026-01-03T10:00:00Z')").await.unwrap();
        let err = delete_from(&client, &schema, "items", &[key(1, 1), key(3, 3)]).await.unwrap_err();
        assert!(err.contains("foreign key"), "{err}");
        let left: i64 = client.query_one("SELECT count(*) FROM items", &[]).await.unwrap().get(0);
        assert_eq!(left, 3);

        assert_eq!(delete_from(&client, &schema, "items", &[key(1, 1), key(2, 2)]).await.unwrap(), 2);
        let left: i64 = client.query_one("SELECT count(*) FROM items", &[]).await.unwrap().get(0);
        assert_eq!(left, 1);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn materialized_views_have_valid_columns_for_sorting_and_filters() {
        let client = connect().await;
        client
            .batch_execute(
                "DROP SCHEMA IF EXISTS dbunny_sort_test CASCADE; CREATE SCHEMA dbunny_sort_test; \
                 CREATE MATERIALIZED VIEW dbunny_sort_test.mv AS SELECT g AS n, g % 2 AS parity FROM generate_series(1, 6) g;",
            )
            .await
            .unwrap();
        let mut columns = get_valid_columns(&client, "dbunny_sort_test", "mv").await.unwrap();
        columns.sort();
        assert_eq!(columns, vec!["n", "parity"]);

        let order = order_clause(&[SortKey { column: "parity".into(), direction: None }, SortKey { column: "n".into(), direction: Some("desc".into()) }], &columns);
        let page = select_table_page(&client, "\"dbunny_sort_test\".\"mv\"", "", &[], &order, 10, 0).await.unwrap();
        let first: Vec<_> = page.rows.iter().map(|r| r[0].clone()).collect();
        assert_eq!(first, vec![json!(6), json!(4), json!(2), json!(5), json!(3), json!(1)]);
        client.batch_execute("DROP SCHEMA dbunny_sort_test CASCADE").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn imports_are_converted_by_the_server_and_all_or_nothing() {
        let client = connect().await;
        client
            .batch_execute("CREATE TEMP TABLE imported (id int PRIMARY KEY, name text NOT NULL, born date, score numeric)")
            .await
            .unwrap();
        let schema: String = client.query_one("SELECT nspname::text FROM pg_namespace WHERE oid = pg_my_temp_schema()", &[]).await.unwrap().get(0);
        let cols: Vec<String> = vec!["id".into(), "name".into(), "born".into(), "score".into()];
        let row = |id: i32, name: &str, born: Option<&str>| vec![Some(id.to_string()), Some(name.to_string()), born.map(String::from), Some("1.5".into())];

        let rows: Vec<_> = (1..=1500).map(|i| row(i, &format!("person {i}"), Some("1990-05-17"))).collect();
        assert_eq!(import_into(&client, &schema, "imported", &cols, &rows).await.unwrap(), 1500);

        // A duplicate key in the second batch rolls back the first one too
        let mut bad: Vec<_> = (2001..=3200).map(|i| row(i, "x", None)).collect();
        bad.push(row(1, "duplicate", None));
        let err = import_into(&client, &schema, "imported", &cols, &bad).await.unwrap_err();
        assert!(err.contains("Nothing was imported") && err.contains("duplicate key"), "{err}");
        let total: i64 = client.query_one("SELECT count(*) FROM imported", &[]).await.unwrap().get(0);
        assert_eq!(total, 1500);

        let err = import_into(&client, &schema, "imported", &cols, &[row(9999, "y", Some("not a date"))]).await.unwrap_err();
        assert!(err.contains("invalid input syntax for type date"), "{err}");
        // The connection is usable after a failed import
        let one: i32 = client.query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(one, 1);
    }
}
