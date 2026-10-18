//! A MySQL table's rows: paging, counting and editing. Saving and importing run in one
//! `START TRANSACTION` on a connection of the pool, so all of it is written or none is;
//! a table whose engine can't undo changes says how many rows were already saved instead.

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Row, Value};
use serde_json::Value as JsonValue;

use super::catalog::{column_definitions, number, qualified, quote, table_engine};
use super::MysqlDriver;
use crate::db::model::{JsonMap, QueryColumn, RowChange, TableDataParams, TableDataResponse};
use crate::db::mysql_common::errors::error_message;
use crate::db::mysql_common::values::{kind_of, type_name, value_to_json, MysqlBinder};
use crate::db::sql::edit::{build_delete_in, build_import_batches_in, build_insert_in, build_update_in};
use crate::db::sql::filter::build_where_in;
use crate::db::sql::order::{order_clause_in, sort_keys};
use crate::db::sql::Dialect;
use crate::db::Data;

const MYSQL: Dialect = Dialect::Mysql;

/// From this many rows on the table statistics stand in for a count, which would be slow
/// and, for InnoDB, no more exact than the statistics themselves.
const ESTIMATE_THRESHOLD: i64 = 100_000;

/// Keys deleted by one statement: each key adds an OR to the condition.
const DELETE_BATCH: usize = 250;

/// The `DATA_TYPE`s of spatial columns: read as text, never written by the grid.
const GEOMETRY_TYPES: [&str; 8] =
    ["geometry", "point", "linestring", "polygon", "multipoint", "multilinestring", "multipolygon", "geometrycollection"];

/// Whether `engine` undoes a failed transaction. A view has none, and counts as one that does.
fn is_transactional(engine: Option<&str>) -> bool {
    !matches!(
        engine.map(str::to_ascii_uppercase).as_deref(),
        Some("MYISAM" | "MEMORY" | "HEAP" | "CSV" | "ARCHIVE" | "MRG_MYISAM" | "BLACKHOLE" | "FEDERATED")
    )
}

/// What statements on a table may name, and how its columns take values.
struct TableShape {
    /// The names of the columns, in order.
    columns: Vec<String>,
    /// The ones that hold geometry.
    geometry: Vec<String>,
    /// What a page selects: every column, geometry as WKT under its own name.
    select_list: String,
    binder: MysqlBinder,
}

impl TableShape {
    async fn read(conn: &mut Conn, schema: &str, table: &str) -> Result<TableShape, String> {
        let definitions = column_definitions(conn, schema, table).await?;
        if definitions.is_empty() {
            return Err(format!("Table '{schema}.{table}' doesn't exist (MySQL error 1146)"));
        }
        let mut shape = TableShape {
            columns: Vec::new(),
            geometry: Vec::new(),
            select_list: String::new(),
            binder: MysqlBinder { kinds: Default::default() },
        };
        let mut selected = Vec::new();
        for definition in definitions {
            let name = quote(&definition.name);
            if GEOMETRY_TYPES.contains(&definition.data_type.to_ascii_lowercase().as_str()) {
                selected.push(format!("ST_AsText({name}) AS {name}"));
                shape.geometry.push(definition.name.clone());
            } else {
                selected.push(name);
            }
            shape.binder.kinds.insert(definition.name.clone(), kind_of(&definition.data_type, definition.charset.as_deref()));
            shape.columns.push(definition.name);
        }
        shape.select_list = selected.join(", ");
        Ok(shape)
    }

    /// Geometry is only read: a write that names a geometry column is refused.
    fn refuse_geometry<'a>(&self, mut columns: impl Iterator<Item = &'a String>) -> Result<(), String> {
        match columns.find(|column| self.geometry.contains(column)) {
            Some(column) => Err(format!("{column} is a geometry column, which can't be edited here")),
            None => Ok(()),
        }
    }
}

/// "{n} of {total} rows were already {done}: the table uses {engine}, which can't undo changes."
fn already_done(n: usize, total: usize, done: &str, engine: Option<&str>) -> String {
    format!("{n} of {total} rows were already {done}: the table uses {}, which can't undo changes.", engine.unwrap_or("an engine that"))
}

impl MysqlDriver {
    /// An error of the server in the app's words.
    fn failure(&self, e: &mysql_async::Error) -> String {
        error_message(e, self.endpoint())
    }

    /// Runs one statement and says how many rows it matched (the connection counts matched
    /// rows, not only the ones that changed).
    async fn run(&self, conn: &mut Conn, sql: &str, params: Vec<Value>) -> Result<u64, String> {
        conn.exec_drop(sql, params).await.map_err(|e| self.failure(&e))?;
        Ok(conn.affected_rows())
    }

    async fn count(&self, conn: &mut Conn, relation: &str, where_sql: &str, params: Vec<Value>) -> Result<i64, String> {
        let count: Option<i64> =
            conn.exec_first(format!("SELECT COUNT(*) FROM {relation}{where_sql}"), params).await.map_err(|e| self.failure(&e))?;
        Ok(count.unwrap_or(0))
    }

    /// The row count from the table statistics, or None for a view, a table the statistics
    /// don't know and anything small enough to just count.
    async fn estimated_rows(&self, conn: &mut Conn, schema: &str, table: &str) -> Result<Option<i64>, String> {
        let row: Option<Row> = conn
            .exec_first(
                "SELECT TABLE_ROWS AS table_rows FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE = 'BASE TABLE'",
                (schema, table),
            )
            .await
            .map_err(|e| self.failure(&e))?;
        Ok(row.and_then(|row| number(&row, "table_rows")).filter(|&estimate| estimate >= ESTIMATE_THRESHOLD))
    }
}

#[async_trait]
impl Data for MysqlDriver {
    async fn table_page(&self, schema: &str, table: &str, params: TableDataParams) -> Result<TableDataResponse, String> {
        let mut conn = self.conn().await?;
        let page = params.page.unwrap_or(1).max(1);
        let page_size = params.page_size.unwrap_or(50).clamp(1, 500);
        let offset = (page - 1) * page_size;

        let shape = TableShape::read(&mut conn, schema, table).await?;
        let order = order_clause_in(MYSQL, &sort_keys(&params), &shape.columns);
        let filters = params.filters.as_deref().unwrap_or(&[]);
        let match_any = params.filter_mode.as_deref() == Some("any");
        let (where_sql, where_params) = build_where_in(MYSQL, filters, match_any, &shape.columns, &shape.binder)?;
        let relation = qualified(schema, table);

        // Total rows: exact for small tables or when filtering, estimated for big unfiltered ones
        let estimate = if where_sql.is_empty() { self.estimated_rows(&mut conn, schema, table).await? } else { None };
        let (total_rows, total_rows_estimated) = match estimate {
            Some(estimate) => (estimate, true),
            None => (self.count(&mut conn, &relation, &where_sql, where_params.clone()).await?, false),
        };

        let sql = format!("SELECT {} FROM {relation}{where_sql}{order} LIMIT {page_size} OFFSET {offset}", shape.select_list);
        let result = conn.exec_iter(sql, where_params).await.map_err(|e| self.failure(&e))?;
        let columns = result.columns().unwrap_or_default();
        let rows: Vec<Row> = result.collect_and_drop().await.map_err(|e| self.failure(&e))?;

        let rows = rows
            .iter()
            .map(|row| (0..columns.len()).map(|i| row.as_ref(i).map_or(JsonValue::Null, |value| value_to_json(value, &columns[i]))).collect())
            .collect();
        let columns = columns
            .iter()
            .map(|column| {
                let name = column.name_str().to_string();
                let data_type = if shape.geometry.contains(&name) { "geometry".to_string() } else { type_name(column) };
                QueryColumn { name, data_type }
            })
            .collect();
        Ok(TableDataResponse { columns, rows, total_rows, total_rows_estimated, page, page_size })
    }

    /// Exact number of rows of a table, for when the view only shows an estimate.
    async fn count_rows(&self, schema: &str, table: &str) -> Result<i64, String> {
        let mut conn = self.conn().await?;
        self.count(&mut conn, &qualified(schema, table), "", vec![]).await
    }

    /// Inserts one row. Only the columns present in `values` are written; the rest get
    /// their defaults. A null value is SQL NULL.
    async fn insert_row(&self, schema: &str, table: &str, values: JsonMap) -> Result<u64, String> {
        let mut conn = self.conn().await?;
        let shape = TableShape::read(&mut conn, schema, table).await?;
        shape.refuse_geometry(values.keys())?;
        let relation = qualified(schema, table);
        let (sql, params) = build_insert_in(MYSQL, &relation, &values, &shape.columns, &shape.binder)?;
        // MySQL has no DEFAULT VALUES
        let sql = if values.is_empty() { format!("INSERT INTO {relation} () VALUES ()") } else { sql };
        self.run(&mut conn, &sql, params).await
    }

    /// Each change must find exactly its row: one that was changed or deleted elsewhere
    /// since it was read (or a key that matches several rows) stops the save.
    async fn save_rows(&self, schema: &str, table: &str, changes: Vec<RowChange>) -> Result<u64, String> {
        let mut conn = self.conn().await?;
        let shape = TableShape::read(&mut conn, schema, table).await?;
        let relation = qualified(schema, table);
        let total = changes.len();
        let which = |i: usize| if total > 1 { format!("Row {} of {total}", i + 1) } else { "The row".to_string() };

        // Everything is checked before anything is written, so a table that can't undo
        // changes isn't left half-saved by a mistake that was there from the start
        let mut statements = Vec::with_capacity(total);
        for (i, change) in changes.iter().enumerate() {
            let built = shape
                .refuse_geometry(change.updates.keys().chain(change.primary_key.keys()))
                .and_then(|_| build_update_in(MYSQL, &relation, &change.primary_key, &change.updates, &shape.columns, &shape.binder));
            statements.push(built.map_err(|e| format!("Nothing was saved. {}: {e}", which(i)))?);
        }

        let engine = table_engine(&mut conn, schema, table).await?;
        let atomic = is_transactional(engine.as_deref());
        conn.query_drop("START TRANSACTION").await.map_err(|e| format!("Nothing was saved: {}", self.failure(&e)))?;
        for (i, (sql, params)) in statements.into_iter().enumerate() {
            let failure = match self.run(&mut conn, &sql, params).await {
                Ok(1) => continue,
                Ok(0) => "it wasn't found, so it was changed or deleted since it was loaded".to_string(),
                Ok(n) => format!("its key matched {n} rows"),
                Err(e) => e,
            };
            let _ = conn.query_drop("ROLLBACK").await;
            return Err(if atomic || i == 0 {
                format!("Nothing was saved. {}: {failure}", which(i))
            } else {
                format!("{} {}: {failure}", already_done(i, total, "saved", engine.as_deref()), which(i))
            });
        }
        if let Err(e) = conn.query_drop("COMMIT").await {
            let _ = conn.query_drop("ROLLBACK").await;
            return Err(format!("Nothing was saved: {}", self.failure(&e)));
        }
        Ok(total as u64)
    }

    async fn delete_rows(&self, schema: &str, table: &str, keys: Vec<JsonMap>) -> Result<u64, String> {
        let mut conn = self.conn().await?;
        let shape = TableShape::read(&mut conn, schema, table).await?;
        for key in &keys {
            shape.refuse_geometry(key.keys())?;
        }
        let relation = qualified(schema, table);
        // Every key is checked before anything runs; no keys at all gets the builder's own error
        let batches: Vec<&[JsonMap]> = if keys.is_empty() { vec![&[]] } else { keys.chunks(DELETE_BATCH).collect() };
        let statements = batches
            .into_iter()
            .map(|batch| build_delete_in(MYSQL, &relation, batch, &shape.columns, &shape.binder))
            .collect::<Result<Vec<_>, _>>()?;
        if statements.len() == 1 {
            let (sql, params) = statements.into_iter().next().expect("one statement");
            return self.run(&mut conn, &sql, params).await;
        }

        let engine = table_engine(&mut conn, schema, table).await?;
        let atomic = is_transactional(engine.as_deref());
        conn.query_drop("START TRANSACTION").await.map_err(|e| self.failure(&e))?;
        let mut deleted = 0;
        for (i, (sql, params)) in statements.into_iter().enumerate() {
            match self.run(&mut conn, &sql, params).await {
                Ok(n) => deleted += n,
                Err(e) => {
                    let _ = conn.query_drop("ROLLBACK").await;
                    return Err(if atomic || i == 0 {
                        e
                    } else {
                        format!("{} {e}", already_done(i * DELETE_BATCH, keys.len(), "deleted", engine.as_deref()))
                    });
                }
            }
        }
        if let Err(e) = conn.query_drop("COMMIT").await {
            let _ = conn.query_drop("ROLLBACK").await;
            return Err(self.failure(&e));
        }
        Ok(deleted)
    }

    /// Inserts imported rows (e.g. from a CSV file) all at once: a bad row leaves the
    /// table untouched. A table that can't undo changes takes its rows one by one, so the
    /// message knows how many were saved.
    async fn import_rows(
        &self,
        schema: &str,
        table: &str,
        columns: Vec<String>,
        rows: Vec<Vec<Option<String>>>,
    ) -> Result<u64, String> {
        let mut conn = self.conn().await?;
        let shape = TableShape::read(&mut conn, schema, table).await?;
        shape.refuse_geometry(columns.iter())?;
        let relation = qualified(schema, table);
        let mut batches = build_import_batches_in(MYSQL, &relation, &columns, &rows, &shape.columns, &shape.binder)?;

        let engine = table_engine(&mut conn, schema, table).await?;
        let atomic = is_transactional(engine.as_deref());
        if !atomic {
            batches = (0..rows.len())
                .map(|i| {
                    let (_, sql, params) = build_import_batches_in(MYSQL, &relation, &columns, &rows[i..=i], &shape.columns, &shape.binder)?
                        .remove(0);
                    Ok((i, sql, params))
                })
                .collect::<Result<_, String>>()?;
        }

        conn.query_drop("START TRANSACTION").await.map_err(|e| format!("Nothing was imported: {}", self.failure(&e)))?;
        let mut inserted = 0;
        for (first_row, sql, params) in batches {
            let size = params.len() / columns.len();
            match self.run(&mut conn, &sql, params).await {
                Ok(n) => inserted += n,
                Err(e) => {
                    let _ = conn.query_drop("ROLLBACK").await;
                    return Err(if atomic {
                        format!("Nothing was imported. A row between {} and {} failed: {e}", first_row + 1, first_row + size)
                    } else if first_row == 0 {
                        format!("Nothing was imported. Row 1 failed: {e}")
                    } else {
                        format!("{} Row {} failed: {e}", already_done(first_row, rows.len(), "saved", engine.as_deref()), first_row + 1)
                    });
                }
            }
        }
        if let Err(e) = conn.query_drop("COMMIT").await {
            let _ = conn.query_drop("ROLLBACK").await;
            return Err(format!("Nothing was imported: {}", self.failure(&e)));
        }
        Ok(inserted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_engines_that_cannot_undo_changes_are_called_out() {
        for engine in ["InnoDB", "NDBCLUSTER"] {
            assert!(is_transactional(Some(engine)), "{engine}");
        }
        for engine in ["MyISAM", "MEMORY", "CSV", "ARCHIVE", "MRG_MYISAM", "myisam"] {
            assert!(!is_transactional(Some(engine)), "{engine}");
        }
        // A view has no engine
        assert!(is_transactional(None));
    }

    #[test]
    fn the_unsaved_rows_message_names_the_engine() {
        assert_eq!(
            already_done(3, 5, "saved", Some("MyISAM")),
            "3 of 5 rows were already saved: the table uses MyISAM, which can't undo changes."
        );
    }
}

/// Tests that need a MySQL server, like the ones of the session (see `mod.rs`).
#[cfg(test)]
mod server_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use mysql_async::prelude::Queryable;
    use serde_json::{json, Value as JsonValue};

    use super::super::tests_support::{exec, query_one, TestDb};
    use super::super::MysqlDriver;
    use crate::db::model::{JsonMap, RowChange, SortKey, TableDataParams, TableDataResponse};
    use crate::db::mysql_common::errors::LOCKED;
    use crate::db::sql::filter::TableFilter;
    use crate::db::Driver;

    const SHOP: &str = include_str!("fixtures/shop.sql");

    /// A database with the shop in it, and a session open on it.
    async fn shop() -> (TestDb, Arc<MysqlDriver>) {
        let db = TestDb::new().await;
        exec(&db.endpoint, SHOP).await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        (db, driver)
    }

    fn map(pairs: JsonValue) -> JsonMap {
        pairs.as_object().unwrap().clone()
    }

    fn change(key: JsonValue, updates: JsonValue) -> RowChange {
        RowChange { primary_key: map(key), updates: map(updates) }
    }

    fn filter(column: &str, operator: &str, value: &str) -> TableFilter {
        TableFilter { column: column.into(), operator: operator.into(), value: value.into() }
    }

    fn sort(column: &str, direction: &str) -> SortKey {
        SortKey { column: column.into(), direction: Some(direction.into()) }
    }

    fn first_column(page: &TableDataResponse) -> Vec<JsonValue> {
        page.rows.iter().map(|r| r[0].clone()).collect()
    }

    async fn page(driver: &MysqlDriver, db: &TestDb, table: &str, params: TableDataParams) -> TableDataResponse {
        driver.data().table_page(&db.name, table, params).await.unwrap()
    }

    /// Customers `user01@shop.test` ... with ids and ages 1 to `n`.
    async fn customers(db: &TestDb, n: usize) {
        let rows: Vec<String> = (1..=n).map(|i| format!("('user{i:02}@shop.test', 'name{i:02}', {i})")).collect();
        exec(&db.endpoint, &format!("INSERT INTO customers (email, name, age) VALUES {}", rows.join(", "))).await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_page_sorts_filters_and_counts() {
        let (db, driver) = shop().await;
        customers(&db, 25).await;

        let paged = |n: i64| TableDataParams { page: Some(n), page_size: Some(10), ..Default::default() };
        let first = page(&driver, &db, "customers", paged(1)).await;
        let names: Vec<&str> = first.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "email", "name", "age", "bio"]);
        let types: Vec<&str> = first.columns.iter().map(|c| c.data_type.as_str()).collect();
        assert_eq!(types, vec!["int", "varchar", "varchar", "int", "text"]);
        assert_eq!((first.total_rows, first.total_rows_estimated, first.page, first.page_size), (25, false, 1, 10));
        assert_eq!(first.rows[0], vec![json!(1), json!("user01@shop.test"), json!("name01"), json!(1), JsonValue::Null]);
        assert_eq!(first_column(&first), (1..=10).map(|i| json!(i)).collect::<Vec<_>>());
        let second = page(&driver, &db, "customers", paged(2)).await;
        assert_eq!(first_column(&second), (11..=20).map(|i| json!(i)).collect::<Vec<_>>());
        let third = page(&driver, &db, "customers", paged(3)).await;
        assert_eq!((first_column(&third), third.total_rows), ((21..=25).map(|i| json!(i)).collect::<Vec<_>>(), 25));

        // Descending, and the single sort column of older callers; page and size are clamped
        let descending = TableDataParams { sort: Some(vec![sort("nope", "asc"), sort("id", "desc")]), page_size: Some(3), ..Default::default() };
        assert_eq!(first_column(&page(&driver, &db, "customers", descending).await), vec![json!(25), json!(24), json!(23)]);
        let legacy = TableDataParams {
            sort_column: Some("id".into()),
            sort_direction: Some("desc".into()),
            page: Some(0),
            page_size: Some(10_000),
            ..Default::default()
        };
        let legacy = page(&driver, &db, "customers", legacy).await;
        assert_eq!((legacy.page, legacy.page_size, legacy.rows[0][0].clone()), (1, 500, json!(25)));

        let filtered = |filters: Vec<TableFilter>, any: bool| TableDataParams {
            filters: Some(filters),
            filter_mode: any.then(|| "any".to_string()),
            sort: Some(vec![sort("id", "asc")]),
            ..Default::default()
        };
        let like = page(&driver, &db, "customers", filtered(vec![filter("email", "like", "user1%")], false)).await;
        assert_eq!((like.rows.len(), like.total_rows, like.total_rows_estimated), (10, 10, false));
        let ilike = page(&driver, &db, "customers", filtered(vec![filter("email", "ilike", "USER2%")], false)).await;
        assert_eq!(ilike.total_rows, 6);
        let listed = page(&driver, &db, "customers", filtered(vec![filter("id", "in", "3, 5")], false)).await;
        assert_eq!((first_column(&listed), listed.total_rows), (vec![json!(3), json!(5)], 2));
        let not_in = page(&driver, &db, "customers", filtered(vec![filter("id", "not in", "1,2,3")], false)).await;
        assert_eq!(not_in.total_rows, 22);
        let nulls = page(&driver, &db, "customers", filtered(vec![filter("bio", "is null", "")], false)).await;
        assert_eq!(nulls.total_rows, 25);
        let any = page(&driver, &db, "customers", filtered(vec![filter("id", "=", "1"), filter("id", "=", "25")], true)).await;
        assert_eq!(first_column(&any), vec![json!(1), json!(25)]);

        // Parameters bind in the order of their placeholders, several operators at once
        let many = filtered(
            vec![
                filter("email", "like", "user1%"),
                filter("age", ">=", "15"),
                filter("id", "in", "11,12,13,14,15,16,17"),
                filter("name", "!=", "name16"),
                filter("age", "<", "18"),
            ],
            false,
        );
        let many = page(&driver, &db, "customers", many).await;
        assert_eq!((first_column(&many), many.total_rows), (vec![json!(15), json!(17)], 2));

        assert_eq!(driver.data().count_rows(&db.name, "customers").await.unwrap(), 25);
        let err = driver.data().table_page(&db.name, "customers", filtered(vec![filter("nope", "=", "1")], false)).await.err().unwrap();
        assert_eq!(err, "Invalid column: nope");
        let err = driver.data().table_page(&db.name, "missing", TableDataParams::default()).await.err().unwrap();
        assert!(err.contains("doesn't exist"), "{err}");

        // A view pages and counts like a table
        exec(&db.endpoint, "INSERT INTO orders (customer_id, total) VALUES (1, 150), (1, 20)").await;
        let view = page(&driver, &db, "big_orders", TableDataParams::default()).await;
        assert_eq!((view.rows.len(), view.total_rows, view.total_rows_estimated), (1, 1, false));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn values_come_back_in_the_shapes_the_grid_knows() {
        let (db, driver) = shop().await;
        customers(&db, 1).await;
        exec(&db.endpoint, "INSERT INTO orders (customer_id, status, total, placed_at) VALUES (1, 'paid', 12.50, '2026-01-02 03:04:05')").await;
        let orders = page(&driver, &db, "orders", TableDataParams::default()).await;
        let by_name = |name: &str| orders.columns.iter().position(|c| c.name == name).unwrap();
        let row = &orders.rows[0];
        assert_eq!(row[by_name("status")], json!("paid"));
        assert_eq!(row[by_name("total")], json!("12.50"));
        assert_eq!(row[by_name("total_with_tax")], json!("15.00"));
        assert_eq!(row[by_name("placed_at")], json!("2026-01-02 03:04:05"));
        assert_eq!(orders.columns[by_name("total")].data_type, "decimal");
        assert_eq!(orders.columns[by_name("status")].data_type, "enum");
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn saving_an_unchanged_value_counts_the_row() {
        let (db, driver) = shop().await;
        customers(&db, 1).await;
        // The same email that is already there: MySQL would count 0 rows without CLIENT_FOUND_ROWS
        let same = change(json!({"id": 1}), json!({"email": "user01@shop.test"}));
        assert_eq!(driver.data().save_rows(&db.name, "customers", vec![same]).await, Ok(1));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn saving_binds_set_values_before_key_values() {
        let (db, driver) = shop().await;
        customers(&db, 3).await;
        exec(&db.endpoint, "CREATE TABLE pairs (a INT, b INT, x VARCHAR(10), y VARCHAR(10), z INT, PRIMARY KEY (a, b))").await;
        exec(&db.endpoint, "INSERT INTO pairs VALUES (1, 2, 'x1', 'y1', 0), (2, 1, 'x2', 'y2', 0)").await;
        let update = change(json!({"a": 2, "b": 1}), json!({"x": "new x", "y": "new y", "z": 7}));
        assert_eq!(driver.data().save_rows(&db.name, "pairs", vec![update]).await, Ok(1));
        let seen: Option<(String, String, i32)> = query_one(&db.endpoint, "SELECT x, y, z FROM pairs WHERE a = 2 AND b = 1").await;
        assert_eq!(seen, Some(("new x".into(), "new y".into(), 7)));
        let untouched: Option<(String, String, i32)> = query_one(&db.endpoint, "SELECT x, y, z FROM pairs WHERE a = 1").await;
        assert_eq!(untouched, Some(("x1".into(), "y1".into(), 0)));

        // A row that isn't there, and the row of someone else's key: the same words as PostgreSQL
        let gone = change(json!({"a": 9, "b": 9}), json!({"z": 1}));
        let err = driver.data().save_rows(&db.name, "pairs", vec![gone]).await.err().unwrap();
        assert_eq!(err, "Nothing was saved. The row: it wasn't found, so it was changed or deleted since it was loaded");
        let invalid = change(json!({"a": 1, "b": 2}), json!({"nope": 1}));
        let err = driver.data().save_rows(&db.name, "pairs", vec![invalid]).await.err().unwrap();
        assert_eq!(err, "Nothing was saved. The row: Invalid column: nope");
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn saving_is_all_or_nothing_on_innodb() {
        let (db, driver) = shop().await;
        customers(&db, 2).await;
        let changes = vec![
            change(json!({"id": 1}), json!({"name": "changed"})),
            // The second email is the first customer's: it breaks the UNIQUE key
            change(json!({"id": 2}), json!({"email": "user01@shop.test"})),
        ];
        let err = driver.data().save_rows(&db.name, "customers", changes).await.err().unwrap();
        assert!(err.starts_with("Nothing was saved. Row 2 of 2:"), "{err}");
        assert!(err.contains("Duplicate entry"), "{err}");
        let name: Option<String> = query_one(&db.endpoint, "SELECT name FROM customers WHERE id = 1").await;
        assert_eq!(name.as_deref(), Some("name01"), "the first change must not stay");

        // Both fine: both saved
        let both = vec![
            change(json!({"id": 1}), json!({"name": "one"})),
            change(json!({"id": 2}), json!({"name": "two"})),
        ];
        assert_eq!(driver.data().save_rows(&db.name, "customers", both).await, Ok(2));
        let names: Option<String> = query_one(&db.endpoint, "SELECT GROUP_CONCAT(name ORDER BY id) FROM customers").await;
        assert_eq!(names.as_deref(), Some("one,two"));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_myisam_table_says_what_was_already_saved() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE m (id INT PRIMARY KEY, v VARCHAR(10) UNIQUE) ENGINE=MyISAM").await;
        exec(&db.endpoint, "INSERT INTO m VALUES (1, 'x'), (2, 'y')").await;
        let changes = vec![change(json!({"id": 1}), json!({"v": "a"})), change(json!({"id": 2}), json!({"v": "a"}))];
        let err = driver.data().save_rows(&db.name, "m", changes).await.err().unwrap();
        let said = "1 of 2 rows were already saved: the table uses MyISAM, which can't undo changes.";
        assert!(err.starts_with(said), "{err}");
        assert!(err.contains("Row 2 of 2: ") && err.contains("Duplicate entry"), "{err}");
        let v: Option<String> = query_one(&db.endpoint, "SELECT v FROM m WHERE id = 1").await;
        assert_eq!(v.as_deref(), Some("a"), "MyISAM can't undo the first change");

        // The first row failing saved nothing, and says so
        let first_fails = vec![change(json!({"id": 2}), json!({"v": "a"})), change(json!({"id": 1}), json!({"v": "b"}))];
        let err = driver.data().save_rows(&db.name, "m", first_fails).await.err().unwrap();
        assert!(err.starts_with("Nothing was saved. Row 1 of 2: "), "{err}");

        // An import row by row: the rows before the bad one stay
        let rows = vec![vec![Some("5".to_string()), Some("p".to_string())], vec![Some("6".to_string()), Some("a".to_string())]];
        let err = driver.data().import_rows(&db.name, "m", vec!["id".into(), "v".into()], rows).await.err().unwrap();
        assert!(err.starts_with("1 of 2 rows were already saved: the table uses MyISAM, which can't undo changes."), "{err}");
        let count: Option<i64> = query_one(&db.endpoint, "SELECT COUNT(*) FROM m").await;
        assert_eq!(count, Some(3));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn insert_delete_and_import_work() {
        let (db, driver) = shop().await;
        let data = driver.data();

        // The auto_increment id is left out
        let inserted = data.insert_row(&db.name, "customers", map(json!({"email": "new@shop.test", "age": null}))).await;
        assert_eq!(inserted, Ok(1));
        let id: Option<i64> = query_one(&db.endpoint, "SELECT id FROM customers WHERE email = 'new@shop.test'").await;
        assert_eq!(id, Some(1));
        let err = data.insert_row(&db.name, "customers", map(json!({"nope": 1}))).await.err().unwrap();
        assert_eq!(err, "Invalid column: nope");
        // No values at all inserts a row of defaults
        exec(&db.endpoint, "CREATE TABLE defaults (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(10) DEFAULT 'none')").await;
        assert_eq!(data.insert_row(&db.name, "defaults", JsonMap::new()).await, Ok(1));
        let label: Option<String> = query_one(&db.endpoint, "SELECT label FROM defaults").await;
        assert_eq!(label.as_deref(), Some("none"));

        // Two keys, one statement
        customers(&db, 3).await;
        let keys = vec![map(json!({"id": 2})), map(json!({"id": 3}))];
        assert_eq!(data.delete_rows(&db.name, "customers", keys).await, Ok(2));
        let left: Option<i64> = query_one(&db.endpoint, "SELECT COUNT(*) FROM customers").await;
        assert_eq!(left, Some(2));
        assert_eq!(data.delete_rows(&db.name, "customers", vec![]).await.err().unwrap(), "primaryKeys is required");

        // Three imported rows, with NULL in them
        let rows = vec![
            vec![Some("a@import.test".to_string()), Some("Ann".to_string()), Some("30".to_string())],
            vec![Some("b@import.test".to_string()), None, None],
            vec![Some("c@import.test".to_string()), Some("".to_string()), Some("0".to_string())],
        ];
        let columns = vec!["email".to_string(), "bio".to_string(), "age".to_string()];
        assert_eq!(data.import_rows(&db.name, "customers", columns.clone(), rows).await, Ok(3));
        let seen: Option<(Option<String>, Option<i32>)> = query_one(&db.endpoint, "SELECT bio, age FROM customers WHERE email = 'b@import.test'").await;
        assert_eq!(seen, Some((None, None)));
        let empty: Option<String> = query_one(&db.endpoint, "SELECT bio FROM customers WHERE email = 'c@import.test'").await;
        assert_eq!(empty.as_deref(), Some(""));

        // One bad row and the whole import is undone
        let rows = vec![
            vec![Some("d@import.test".to_string()), None, None],
            vec![Some("a@import.test".to_string()), None, None],
        ];
        let err = data.import_rows(&db.name, "customers", columns, rows).await.err().unwrap();
        assert!(err.starts_with("Nothing was imported. A row between 1 and 2 failed: "), "{err}");
        let d: Option<i64> = query_one(&db.endpoint, "SELECT COUNT(*) FROM customers WHERE email = 'd@import.test'").await;
        assert_eq!(d, Some(0));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn binary_and_bit_columns_round_trip() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE raw (id INT PRIMARY KEY, b VARBINARY(8), flags BIT(8), data BLOB)").await;
        let data = driver.data();
        let insert = map(json!({"id": 1, "b": "\\x00ff10", "flags": 5, "data": "plain"}));
        assert_eq!(data.insert_row(&db.name, "raw", insert).await, Ok(1));
        let stored: Option<(String, u8, String)> = query_one(&db.endpoint, "SELECT HEX(b), flags + 0, CAST(data AS CHAR) FROM raw").await;
        assert_eq!(stored, Some(("00FF10".into(), 5, "plain".into())));
        let shown = page(&driver, &db, "raw", TableDataParams::default()).await;
        assert_eq!(shown.rows[0], vec![json!(1), json!("\\x00ff10"), json!(5), json!("\\x706c61696e")]);

        // Found again by the same value
        let update = change(json!({"id": 1, "b": "\\x00ff10"}), json!({"flags": 6}));
        assert_eq!(data.save_rows(&db.name, "raw", vec![update]).await, Ok(1));
        let found = page(&driver, &db, "raw", TableDataParams { filters: Some(vec![filter("b", "=", "\\x00ff10")]), ..Default::default() }).await;
        assert_eq!((found.total_rows, found.rows[0][2].clone()), (1, json!(6)));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn an_open_editor_transaction_makes_saving_fail_fast() {
        let (db, driver) = shop().await;
        customers(&db, 1).await;
        driver
            .editor()
            .run(|c| {
                Box::pin(async move {
                    c.query_drop("START TRANSACTION").await.map_err(|e| e.to_string())?;
                    c.query_drop("UPDATE customers SET name = 'held' WHERE id = 1").await.map_err(|e| e.to_string())
                })
            })
            .await
            .unwrap();

        let started = Instant::now();
        let blocked = change(json!({"id": 1}), json!({"name": "mine"}));
        let err = driver.data().save_rows(&db.name, "customers", vec![blocked]).await.err().unwrap();
        assert!(started.elapsed() < Duration::from_secs(10), "waited {:?}", started.elapsed());
        assert_eq!(err, format!("Nothing was saved. The row: {LOCKED}"));

        // Deleting the held row waits the same few seconds, and says the same
        let err = driver.data().delete_rows(&db.name, "customers", vec![map(json!({"id": 1}))]).await.err().unwrap();
        assert_eq!(err, LOCKED);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn geometry_columns_read_as_wkt() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE places (id INT PRIMARY KEY, spot POINT, area POLYGON NULL)").await;
        exec(&db.endpoint, "INSERT INTO places VALUES (1, ST_GeomFromText('POINT(1 2)'), NULL)").await;
        let shown = page(&driver, &db, "places", TableDataParams::default()).await;
        let names: Vec<&str> = shown.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "spot", "area"]);
        assert_eq!(shown.rows[0], vec![json!(1), json!("POINT(1 2)"), JsonValue::Null]);
        assert_eq!(shown.columns[1].data_type, "geometry");

        // The grid can't write them, and the data path says so instead of corrupting the column
        let err = driver.data().save_rows(&db.name, "places", vec![change(json!({"id": 1}), json!({"spot": "POINT(3 4)"}))]).await.err().unwrap();
        assert_eq!(err, "Nothing was saved. The row: spot is a geometry column, which can't be edited here");
        let err = driver.data().insert_row(&db.name, "places", map(json!({"id": 2, "area": "x"}))).await.err().unwrap();
        assert_eq!(err, "area is a geometry column, which can't be edited here");
        // Other columns of the same table are still editable
        assert_eq!(driver.data().insert_row(&db.name, "places", map(json!({"id": 3}))).await, Ok(1));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_big_table_reports_an_estimate() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE big (id INT NOT NULL PRIMARY KEY, v INT NOT NULL) ENGINE=InnoDB").await;
        exec(
            &db.endpoint,
            "SET SESSION cte_max_recursion_depth = 300000; \
             INSERT INTO big SELECT n, n % 7 FROM (WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 200000) SELECT n FROM s) t; \
             ANALYZE TABLE big",
        )
        .await;
        let big = page(&driver, &db, "big", TableDataParams { page_size: Some(5), ..Default::default() }).await;
        assert!(big.total_rows_estimated, "200000 rows are estimated");
        assert!((100_000..=400_000).contains(&big.total_rows), "{}", big.total_rows);
        // Filtering counts exactly, and so does the count the view asks for
        let filtered = TableDataParams { filters: Some(vec![filter("v", "=", "3")]), ..Default::default() };
        let filtered = page(&driver, &db, "big", filtered).await;
        assert_eq!((filtered.total_rows, filtered.total_rows_estimated), (28_572, false));
        assert_eq!(driver.data().count_rows(&db.name, "big").await.unwrap(), 200_000);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_float_key_that_cannot_be_matched_says_the_row_was_not_found() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE f (k FLOAT PRIMARY KEY, v INT)").await;
        exec(&db.endpoint, "INSERT INTO f VALUES (0.1, 1)").await;
        let err = driver.data().save_rows(&db.name, "f", vec![change(json!({"k": 0.1}), json!({"v": 2}))]).await.err().unwrap();
        assert_eq!(err, "Nothing was saved. The row: it wasn't found, so it was changed or deleted since it was loaded");
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn a_read_only_session_cannot_write_rows() {
        let (db, _) = shop().await;
        customers(&db, 1).await;
        let endpoint = crate::db::mysql_common::connect::MysqlEndpoint { read_only: true, ..db.endpoint.clone() };
        let driver = MysqlDriver::open(endpoint, None).await.unwrap();
        let err = driver.data().save_rows(&db.name, "customers", vec![change(json!({"id": 1}), json!({"name": "x"}))]).await.err().unwrap();
        assert_eq!(err, "Nothing was saved. The row: This connection is read-only.");
        assert_eq!(driver.data().insert_row(&db.name, "customers", map(json!({"email": "z@z"}))).await.err().unwrap(), "This connection is read-only.");
        driver.close().await;
        db.drop_now().await;
    }
}
