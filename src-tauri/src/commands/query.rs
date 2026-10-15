use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::State;
use tokio_postgres::types::Type;
use tokio_postgres::{Row, SimpleQueryMessage};
use uuid::Uuid;

use super::pgtext::{is_native, text_to_json};
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
    pub rows_affected: Option<u64>,
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

fn pg_error_to_json(e: tokio_postgres::Error) -> String {
    if let Some(db) = e.as_db_error() {
        let position = match db.position() {
            Some(tokio_postgres::error::ErrorPosition::Original(p)) => Some(*p),
            _ => None,
        };
        serde_json::json!({
            "message": db.message(),
            "severity": db.severity(),
            "detail": db.detail(),
            "hint": db.hint(),
            "position": position,
            "code": db.code().code(),
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
    if sql.trim().is_empty() {
        return Err("SQL query is required".into());
    }

    let client = state.client(&session_id).await?;

    let start = std::time::Instant::now();
    let output = run_statement(&client, &sql).await.map_err(pg_error_to_json)?;
    let duration = start.elapsed().as_millis() as u64;

    let row_count = match output.rows_affected {
        Some(affected) => affected as usize,
        None => output.rows.len(),
    };
    Ok(QueryResult {
        columns: output.columns,
        rows: output.rows,
        row_count,
        rows_affected: output.rows_affected,
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
    let client = state.client(&session_id).await?;

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
            let valid_columns = get_valid_columns(&client, &schema, &table).await?;
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
    let count_row = client
        .query_one(&count_sql as &str, &[])
        .await
        .map_err(|e| e.to_string())?;
    let total_rows: i64 = count_row.get(0);

    // Get paginated data
    let data_sql = format!(
        "SELECT * FROM \"{}\".\"{}\"{}  LIMIT {} OFFSET {}",
        schema, table, order_clause, page_size, offset
    );
    let output = run_statement(&client, &data_sql)
        .await
        .map_err(|e| e.to_string())?;

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
}
