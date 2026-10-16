//! A PostgreSQL table's rows: paging, counting and editing.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::Row;
use uuid::Uuid;

use super::PostgresDriver;
use crate::db::model::{JsonMap, QueryColumn, RowChange, TableDataParams, TableDataResponse};
use crate::db::postgres::text::{is_native, text_to_json};
use crate::db::sql::edit::{build_delete, build_import_batches, build_insert, build_update};
use crate::db::sql::filter::{build_where, quote_ident, TextParam};
use crate::db::sql::json::int_to_json;
use crate::db::sql::order::{order_clause, sort_keys};
use crate::db::Data;

fn try_get_string(row: &Row, i: usize) -> JsonValue {
    match row.try_get::<_, Option<String>>(i) {
        Ok(Some(s)) => JsonValue::String(s),
        Ok(None) => JsonValue::Null,
        Err(_) => JsonValue::String("<binary>".to_string()),
    }
}

pub(crate) fn row_to_json_values(row: &Row) -> Vec<JsonValue> {
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

pub(crate) fn columns_from_statement(columns: &[tokio_postgres::Column]) -> Vec<QueryColumn> {
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
pub(crate) struct StatementOutput {
    pub(crate) columns: Vec<QueryColumn>,
    pub(crate) rows: Vec<Vec<JsonValue>>,
    /// Set for statements that return no rows (INSERT without RETURNING, DDL...).
    pub(crate) rows_affected: Option<u64>,
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

#[async_trait]
impl Data for PostgresDriver {
    async fn table_page(&self, schema: &str, table: &str, params: TableDataParams) -> Result<TableDataResponse, String> {
        let client = self.client().await?;

        let page = params.page.unwrap_or(1).max(1);
        let page_size = params.page_size.unwrap_or(50).clamp(1, 500);
        let offset = (page - 1) * page_size;

        let valid_columns = get_valid_columns(&client, schema, table).await?;

        let order_clause = order_clause(&sort_keys(&params), &valid_columns);

        // Build WHERE clause from the filters
        let filters = params.filters.as_deref().unwrap_or(&[]);
        let match_any = params.filter_mode.as_deref() == Some("any");
        let (where_sql, where_params) = build_where(filters, match_any, &valid_columns)?;

        let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));

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

    /// Exact number of rows of a table, for when the view only shows an estimate.
    async fn count_rows(&self, schema: &str, table: &str) -> Result<i64, String> {
        let client = self.client().await?;
        let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
        count_rows(&client, &relation, "", &[])
            .await
            .map_err(|e| db_message(&e))
    }

    /// Inserts one row. Only the columns present in `values` are written; the rest get
    /// their defaults. A null value is SQL NULL.
    async fn insert_row(&self, schema: &str, table: &str, values: JsonMap) -> Result<u64, String> {
        let client = self.client().await?;
        let valid_columns = get_valid_columns(&client, schema, table).await?;
        let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
        insert_into(&client, &relation, &values, &valid_columns).await
    }

    /// Saves edited rows all at once: a separate connection runs them in one transaction, so
    /// either every row is saved or none is.
    async fn save_rows(&self, schema: &str, table: &str, changes: Vec<RowChange>) -> Result<u64, String> {
        let link = self.link().await?;
        let notices = self.notices().await?;
        let client = crate::db::postgres::connect::open_connection(&link, &notices)
            .await
            .map_err(|e| format!("Couldn't open a connection to save: {e}"))?;
        save_rows(&client, schema, table, &changes).await
    }

    async fn delete_rows(&self, schema: &str, table: &str, keys: Vec<JsonMap>) -> Result<u64, String> {
        let client = self.client().await?;
        delete_from(&client, schema, table, &keys).await
    }

    /// Inserts imported rows (e.g. from a CSV file) all at once: a separate connection runs
    /// them in one transaction, so a bad row leaves the table untouched.
    async fn import_rows(
        &self,
        schema: &str,
        table: &str,
        columns: Vec<String>,
        rows: Vec<Vec<Option<String>>>,
    ) -> Result<u64, String> {
        let link = self.link().await?;
        let notices = self.notices().await?;
        let client = crate::db::postgres::connect::open_connection(&link, &notices)
            .await
            .map_err(|e| format!("Couldn't open a connection for the import: {e}"))?;
        import_into(&client, schema, table, &columns, &rows).await
    }
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

/// Tests that need a real server, see `console::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::db::postgres::console::run_statement;
    use crate::db::model::SortKey;
    use crate::db::sql::filter::TableFilter;
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
