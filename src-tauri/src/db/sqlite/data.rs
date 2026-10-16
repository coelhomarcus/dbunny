//! A SQLite table's rows: paging, counting and editing. Every edit runs on the tables'
//! connection in its own `BEGIN IMMEDIATE` transaction, so all of it is written or none is.

use async_trait::async_trait;
use rusqlite::types::{ToSql, Value};
use rusqlite::{params_from_iter, Connection};
use serde_json::Value as JsonValue;

use super::catalog::{synthetic_rowid, table_columns};
use super::errors::map_statement_error;
use super::values::{standard_type, value_to_json, Affinity};
use super::SqliteDriver;
use crate::db::model::{JsonMap, QueryColumn, RowChange, TableDataParams, TableDataResponse};
use crate::db::sql::bind::{OperandBinder, ValueBinder};
use crate::db::sql::edit::{build_delete_in, build_import_batches_in, build_insert_in, build_update_in};
use crate::db::sql::filter::build_where_in;
use crate::db::sql::order::{order_clause_in, sort_keys};
use crate::db::sql::Dialect;
use crate::db::Data;

const SQLITE: Dialect = Dialect::Sqlite;

/// Keys deleted by one statement: each key adds an OR, and SQLite limits how deep an
/// expression goes (1000 by default).
const DELETE_BATCH: usize = 250;

#[async_trait]
impl Data for SqliteDriver {
    async fn table_page(&self, schema: &str, table: &str, params: TableDataParams) -> Result<TableDataResponse, String> {
        let page = params.page.unwrap_or(1).max(1);
        let page_size = params.page_size.unwrap_or(50).clamp(1, 500);
        let offset = (page - 1) * page_size;
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let shape = TableShape::read(c, &schema, &table)?;
            let order = order_clause_in(SQLITE, &sort_keys(&params), &shape.columns);
            let filters = params.filters.as_deref().unwrap_or(&[]);
            let match_any = params.filter_mode.as_deref() == Some("any");
            let (where_sql, where_params) =
                build_where_in(SQLITE, filters, match_any, &shape.columns, &shape).map_err(Failure::message)?;
            let relation = relation(&schema, &table);

            let total_rows = count(c, &relation, &where_sql, &where_params)?;

            // A table edited by its rowid shows it first
            let lead = match &shape.rowid {
                Some(name) => format!("{0} AS {0}, ", SQLITE.quote_ident(name)),
                None => String::new(),
            };
            let limit_at = SQLITE.param(where_params.len() + 1);
            let offset_at = SQLITE.param(where_params.len() + 2);
            let sql = format!("SELECT {lead}* FROM {relation}{where_sql}{order} LIMIT {limit_at} OFFSET {offset_at}");
            let mut binds: Vec<&dyn ToSql> = where_params.iter().map(|p| p as &dyn ToSql).collect();
            binds.push(&page_size);
            binds.push(&offset);

            let mut stmt = c.prepare(&sql)?;
            let columns: Vec<QueryColumn> = stmt
                .columns()
                .iter()
                .enumerate()
                .map(|(i, column)| {
                    let declared = if i == 0 && shape.rowid.is_some() { "INTEGER" } else { column.decl_type().unwrap_or("") };
                    QueryColumn { name: column.name().to_string(), data_type: standard_type(declared).to_string() }
                })
                .collect();
            let width = columns.len();
            let mut rows = Vec::new();
            let mut cursor = stmt.query(&*binds)?;
            while let Some(row) = cursor.next()? {
                rows.push((0..width).map(|i| row.get_ref(i).map(value_to_json)).collect::<rusqlite::Result<Vec<_>>>()?);
            }
            Ok(TableDataResponse { columns, rows, total_rows, total_rows_estimated: false, page, page_size })
        })
        .await
    }

    async fn count_rows(&self, schema: &str, table: &str) -> Result<i64, String> {
        let relation = relation(schema, table);
        self.on_tables(move |c| Ok(count(c, &relation, "", &[])?)).await
    }

    /// Inserts one row. Only the columns present in `values` are written; the rest get
    /// their defaults. A null value is SQL NULL.
    async fn insert_row(&self, schema: &str, table: &str, values: JsonMap) -> Result<u64, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let shape = TableShape::read(c, &schema, &table)?;
            let (sql, params) =
                build_insert_in(SQLITE, &relation(&schema, &table), &values, &shape.columns, &shape).map_err(Failure::message)?;
            in_transaction(c, "", |c| Ok(c.execute(&sql, params_from_iter(&params))? as u64))
        })
        .await
    }

    /// Each change must find exactly its row: one that was changed or deleted elsewhere
    /// since it was read (or a key that matches several rows) stops the save.
    async fn save_rows(&self, schema: &str, table: &str, changes: Vec<RowChange>) -> Result<u64, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let shape = TableShape::read(c, &schema, &table)?;
            let relation = relation(&schema, &table);
            let total = changes.len();
            in_transaction(c, "Nothing was saved: ", |c| {
                for (i, change) in changes.iter().enumerate() {
                    let updated = build_update_in(SQLITE, &relation, &change.primary_key, &change.updates, &shape.columns, &shape)
                        .map_err(Failure::message)
                        .and_then(|(sql, params)| Ok(c.execute(&sql, params_from_iter(&params))?));
                    let failure = match updated {
                        Ok(1) => continue,
                        Ok(0) => Failure::message("it wasn't found, so it was changed or deleted since it was loaded".into()),
                        Ok(n) => Failure::message(format!("its key matched {n} rows")),
                        Err(failure) => failure,
                    };
                    let which = if total > 1 { format!("Row {} of {total}", i + 1) } else { "The row".to_string() };
                    return Err(failure.after(&format!("Nothing was saved. {which}: ")));
                }
                Ok(total as u64)
            })
        })
        .await
    }

    async fn delete_rows(&self, schema: &str, table: &str, keys: Vec<JsonMap>) -> Result<u64, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let shape = TableShape::read(c, &schema, &table)?;
            let relation = relation(&schema, &table);
            // Every key is checked before anything runs; no keys at all gets the builder's own error
            let batches: Vec<&[JsonMap]> = if keys.is_empty() { vec![&[]] } else { keys.chunks(DELETE_BATCH).collect() };
            let statements = batches
                .into_iter()
                .map(|batch| build_delete_in(SQLITE, &relation, batch, &shape.columns, &shape))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Failure::message)?;
            in_transaction(c, "", |c| {
                let mut deleted = 0;
                for (sql, params) in &statements {
                    deleted += c.execute(sql, params_from_iter(params))? as u64;
                }
                // A key in a column without a type can match a blob and the text that
                // looks like it; deleting both would delete a row nobody chose
                if deleted > keys.len() as u64 {
                    let which = if keys.len() == 1 { "the key".to_string() } else { format!("the {} keys", keys.len()) };
                    return Err(Failure::message(format!("Nothing was deleted: {which} matched {deleted} rows")));
                }
                Ok(deleted)
            })
        })
        .await
    }

    /// Inserts imported rows (e.g. from a CSV file) all at once: a bad row leaves the
    /// table untouched.
    async fn import_rows(
        &self,
        schema: &str,
        table: &str,
        columns: Vec<String>,
        rows: Vec<Vec<Option<String>>>,
    ) -> Result<u64, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let shape = TableShape::read(c, &schema, &table)?;
            let batches = build_import_batches_in(SQLITE, &relation(&schema, &table), &columns, &rows, &shape.columns, &shape)
                .map_err(Failure::message)?;
            in_transaction(c, "", |c| {
                let mut inserted = 0;
                for (first_row, sql, params) in &batches {
                    match c.execute(sql, params_from_iter(params)) {
                        Ok(count) => inserted += count as u64,
                        Err(e) => {
                            let size = params.len() / columns.len();
                            let context = format!("Nothing was imported. A row between {} and {} failed: ", first_row + 1, first_row + size);
                            return Err(Failure::from(e).after(&context));
                        }
                    }
                }
                Ok(inserted)
            })
        })
        .await
    }
}

impl SqliteDriver {
    /// Runs `f` on the tables' connection, putting a failure in the app's words.
    pub(super) async fn on_tables<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, Failure> + Send + 'static,
    ) -> Result<T, String> {
        match self.tables().run(move |c| Ok(f(c))).await? {
            Ok(value) => Ok(value),
            Err(failure) => Err(self.describe(failure)),
        }
    }

    /// The message of a failure. A busy database is explained by who holds it, which is
    /// the editor when its connection has a transaction open. That is read without
    /// waiting for the editor, which may be running a long statement.
    pub(super) fn describe(&self, failure: Failure) -> String {
        let reason = match failure.cause {
            Cause::Message(message) => message,
            Cause::Sqlite(e) => map_statement_error(&e, self.editor().in_transaction()),
        };
        format!("{}{reason}", failure.context)
    }
}

/// Why a call failed, before it's put in words.
pub(super) struct Failure {
    /// What comes before the reason, such as "Nothing was saved. Row 2 of 3: ".
    context: String,
    cause: Cause,
}

enum Cause {
    Sqlite(rusqlite::Error),
    Message(String),
}

impl Failure {
    pub(super) fn message(message: String) -> Failure {
        Failure { context: String::new(), cause: Cause::Message(message) }
    }

    /// The same failure with `context` before what it already says.
    pub(super) fn after(mut self, context: &str) -> Failure {
        self.context.insert_str(0, context);
        self
    }
}

impl From<rusqlite::Error> for Failure {
    fn from(e: rusqlite::Error) -> Failure {
        Failure { context: String::new(), cause: Cause::Sqlite(e) }
    }
}

/// What statements on a table may name, and how its columns take values.
struct TableShape {
    /// The columns, with the synthetic rowid when the table is edited by it.
    columns: Vec<String>,
    /// The affinity of each of `columns`.
    affinities: Vec<Affinity>,
    /// The synthetic rowid column's name.
    rowid: Option<String>,
}

impl TableShape {
    fn read(c: &Connection, schema: &str, table: &str) -> rusqlite::Result<TableShape> {
        let columns = table_columns(c, schema, table)?;
        let rowid = synthetic_rowid(&columns).map(String::from);
        let affinities = columns.iter().map(|column| Affinity::of(&column.type_name)).collect();
        Ok(TableShape { columns: columns.into_iter().map(|column| column.name).collect(), affinities, rowid })
    }

    fn affinity(&self, column: &str) -> Affinity {
        // The builders check every column before they bind its values
        let at = self.columns.iter().position(|c| c == column);
        at.map(|i| self.affinities[i]).unwrap_or(Affinity::Numeric)
    }
}

/// Values are bound by the column's affinity, since SQLite converts text only for some.
impl ValueBinder for TableShape {
    type Param = Value;

    fn value(&self, column: &str, value: &JsonValue) -> Value {
        self.affinity(column).value(value)
    }

    fn imported(&self, column: &str, value: Option<&str>) -> Value {
        value.map_or(Value::Null, |text| self.affinity(column).text(text))
    }

    fn key(&self, column: &str, value: &JsonValue) -> Vec<Value> {
        self.affinity(column).key(value)
    }
}

impl OperandBinder for TableShape {
    type Param = Value;

    fn text(&self, text: &str) -> Value {
        Value::Text(text.to_string())
    }

    fn compared(&self, column: &str, operand: &str) -> Value {
        self.affinity(column).text(operand)
    }

    fn matched(&self, column: &str, operand: &str) -> Vec<Value> {
        self.affinity(column).matched(operand)
    }
}

/// `"schema"."table"`.
pub(super) fn relation(schema: &str, table: &str) -> String {
    format!("{}.{}", SQLITE.quote_ident(schema), SQLITE.quote_ident(table))
}

fn count(c: &Connection, relation: &str, where_sql: &str, params: &[Value]) -> rusqlite::Result<i64> {
    c.query_row(&format!("SELECT count(*) FROM {relation}{where_sql}"), params_from_iter(params), |row| row.get(0))
}

/// Runs `f` in one `BEGIN IMMEDIATE` transaction: committed when it succeeds, rolled back
/// when it or the commit fails. `commit_context` comes before the reason a commit failed.
pub(super) fn in_transaction<T>(
    c: &Connection,
    commit_context: &str,
    f: impl FnOnce(&Connection) -> Result<T, Failure>,
) -> Result<T, Failure> {
    c.execute_batch("BEGIN IMMEDIATE")?;
    let result = f(c).and_then(|value| {
        c.execute_batch("COMMIT").map_err(|e| Failure::from(e).after(commit_context))?;
        Ok(value)
    });
    // Some errors end the transaction on their own; a failed COMMIT leaves it open
    if result.is_err() && !c.is_autocommit() {
        let _ = c.execute_batch("ROLLBACK");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value as JsonValue};

    use super::super::tests_support::{stop_until_done, without_hanging, TempDb};
    use super::super::OpenOptions;
    use super::*;
    use crate::db::model::SortKey;
    use crate::db::sql::filter::TableFilter;
    use crate::db::{Catalog, Driver};

    async fn open(f: &TempDb) -> Arc<SqliteDriver> {
        SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap()
    }

    fn values(pairs: JsonValue) -> JsonMap {
        pairs.as_object().unwrap().clone()
    }

    fn filter(column: &str, operator: &str, value: &str) -> TableFilter {
        TableFilter { column: column.into(), operator: operator.into(), value: value.into() }
    }

    fn sort(column: &str, direction: &str) -> SortKey {
        SortKey { column: column.into(), direction: Some(direction.into()) }
    }

    fn change(key: JsonValue, updates: JsonValue) -> RowChange {
        RowChange { primary_key: values(key), updates: values(updates) }
    }

    /// Every row of `sql`, as the grid would show it.
    async fn query(d: &SqliteDriver, sql: &str) -> Vec<Vec<JsonValue>> {
        let sql = sql.to_string();
        d.tables()
            .run(move |c| {
                let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
                let n = stmt.column_count();
                let rows = stmt
                    .query_map([], |r| (0..n).map(|i| r.get_ref(i).map(super::super::values::value_to_json)).collect())
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<Vec<JsonValue>>, _>>().map_err(|e| e.to_string())
            })
            .await
            .unwrap()
    }

    async fn page(d: &SqliteDriver, table: &str, params: TableDataParams) -> TableDataResponse {
        d.data().table_page("main", table, params).await.unwrap()
    }

    fn column_names(page: &TableDataResponse) -> Vec<&str> {
        page.columns.iter().map(|c| c.name.as_str()).collect()
    }

    fn first_column(page: &TableDataResponse) -> Vec<JsonValue> {
        page.rows.iter().map(|r| r[0].clone()).collect()
    }

    const PEOPLE: &str = "
        CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
        INSERT INTO people VALUES
            (1, 'ana', 30), (2, 'Andre', 25), (3, 'bia', 30), (4, 'caio', 41),
            (5, 'duda', NULL), (6, 'edu', 19), (7, 'Ana Maria', 25);
    ";

    #[tokio::test]
    async fn pages_sort_and_filter() {
        let f = TempDb::with(PEOPLE);
        let d = open(&f).await;

        let all = page(&d, "people", TableDataParams::default()).await;
        assert_eq!(column_names(&all), vec!["id", "name", "age"]);
        let types: Vec<&str> = all.columns.iter().map(|c| c.data_type.as_str()).collect();
        assert_eq!(types, vec!["bigint", "text", "bigint"]);
        assert_eq!((all.total_rows, all.total_rows_estimated, all.page, all.page_size), (7, false, 1, 50));
        assert_eq!(all.rows[0], vec![json!(1), json!("ana"), json!(30)]);
        assert_eq!(all.rows[4], vec![json!(5), json!("duda"), JsonValue::Null]);

        // Several sort keys, then the second page of three
        let sorted = TableDataParams {
            sort: Some(vec![sort("age", "desc"), sort("nope", "asc"), sort("name", "asc")]),
            page: Some(2),
            page_size: Some(3),
            ..Default::default()
        };
        let second = page(&d, "people", sorted).await;
        assert_eq!(first_column(&second), vec![json!(7), json!(2), json!(6)]);
        assert_eq!((second.total_rows, second.page, second.page_size), (7, 2, 3));

        // The single sort column of older callers, and the clamping of page and size
        let legacy = TableDataParams {
            sort_column: Some("id".into()),
            sort_direction: Some("desc".into()),
            page: Some(0),
            page_size: Some(10_000),
            ..Default::default()
        };
        let legacy = page(&d, "people", legacy).await;
        assert_eq!((legacy.page, legacy.page_size), (1, 500));
        assert_eq!(legacy.rows[0][0], json!(7));

        // Filters: like and ilike both ignore ASCII case; in lists; match any; parameters after them
        let filtered = |filters: Vec<TableFilter>, any: bool| TableDataParams {
            filters: Some(filters),
            filter_mode: any.then(|| "any".to_string()),
            sort: Some(vec![sort("id", "asc")]),
            page_size: Some(2),
            ..Default::default()
        };
        let ana = page(&d, "people", filtered(vec![filter("name", "ilike", "an%")], false)).await;
        assert_eq!((first_column(&ana), ana.total_rows), (vec![json!(1), json!(2)], 3));
        let like = page(&d, "people", filtered(vec![filter("name", "like", "AN%"), filter("age", ">=", "25")], false)).await;
        assert_eq!(like.total_rows, 3);
        let listed = page(&d, "people", filtered(vec![filter("id", "in", "3, 6"), filter("age", "is null", "")], true)).await;
        assert_eq!((first_column(&listed), listed.total_rows), (vec![json!(3), json!(5)], 3));
        let not_in = page(&d, "people", filtered(vec![filter("age", "not in", "25,30")], false)).await;
        assert_eq!(not_in.total_rows, 2, "NULL is neither in nor out of a list");

        assert_eq!(d.data().count_rows("main", "people").await.unwrap(), 7);

        let err = d.data().table_page("main", "people", filtered(vec![filter("nope", "=", "1")], false)).await.err().unwrap();
        assert_eq!(err, "Invalid column: nope");
        let err = d.data().table_page("main", "missing", TableDataParams::default()).await.err().unwrap();
        assert_eq!(err, "no such table: main.missing");
    }

    #[tokio::test]
    async fn filters_compare_numbers_as_numbers() {
        let f = TempDb::with("CREATE TABLE n (id INTEGER PRIMARY KEY, v INTEGER); INSERT INTO n (v) VALUES (9), (10);");
        let d = open(&f).await;
        let params = TableDataParams { filters: Some(vec![filter("v", ">", "9")]), ..Default::default() };
        let found = page(&d, "n", params).await;
        assert_eq!(found.rows, vec![vec![json!(2), json!(10)]]);
        assert_eq!(found.total_rows, 1);
    }

    #[tokio::test]
    async fn big_integers_blobs_and_infinity_round_trip() {
        let f = TempDb::with(
            "CREATE TABLE v (id INTEGER PRIMARY KEY, big INTEGER, data BLOB, r REAL);
             INSERT INTO v VALUES (9007199254740993, 9007199254740993, x'00ff', 1e999),
                                  (9007199254740992, 1, NULL, -1e999);",
        );
        let d = open(&f).await;
        let read = page(&d, "v", TableDataParams { sort: Some(vec![sort("id", "desc")]), ..Default::default() }).await;
        let types: Vec<&str> = read.columns.iter().map(|c| c.data_type.as_str()).collect();
        assert_eq!(types, vec!["bigint", "bigint", "bytea", "double precision"]);
        assert_eq!(read.rows[0], vec![json!("9007199254740993"), json!("9007199254740993"), json!("\\x00ff"), json!("Infinity")]);
        assert_eq!(read.rows[1], vec![json!("9007199254740992"), json!(1), JsonValue::Null, json!("-Infinity")]);

        // The values as the grid holds them find their row and are written back unchanged
        let key = json!({ "id": read.rows[0][0] });
        let updates = json!({ "big": read.rows[0][1], "r": 2.5 });
        assert_eq!(d.data().save_rows("main", "v", vec![change(key, updates)]).await.unwrap(), 1);
        let stored = query(&d, "SELECT id, big, typeof(big), r FROM v ORDER BY id DESC").await;
        assert_eq!(stored[0], vec![json!("9007199254740993"), json!("9007199254740993"), json!("integer"), json!(2.5)]);
        assert_eq!(stored[1], vec![json!("9007199254740992"), json!(1), json!("integer"), json!("-Infinity")], "the neighbour a rounded key would hit");

        // The blob and the infinities the grid shows are written back as a blob and reals
        let back = vec![
            change(json!({ "id": read.rows[0][0] }), json!({ "data": read.rows[0][2], "r": read.rows[0][3] })),
            change(json!({ "id": read.rows[1][0] }), json!({ "data": "\\x", "r": read.rows[1][3] })),
        ];
        assert_eq!(d.data().save_rows("main", "v", back).await.unwrap(), 2);
        let stored = query(&d, "SELECT data, typeof(data), r, typeof(r) FROM v ORDER BY id DESC").await;
        assert_eq!(stored[0], vec![json!("\\x00ff"), json!("blob"), json!("Infinity"), json!("real")]);
        assert_eq!(stored[1], vec![json!("\\x"), json!("blob"), json!("-Infinity"), json!("real")]);
        let reread = page(&d, "v", TableDataParams { sort: Some(vec![sort("id", "desc")]), ..Default::default() }).await;
        assert_eq!(reread.rows[0], read.rows[0]);
        let found = page(&d, "v", TableDataParams { filters: Some(vec![filter("r", "=", "Infinity")]), ..Default::default() }).await;
        assert_eq!(found.total_rows, 1);
        let found = page(&d, "v", TableDataParams { filters: Some(vec![filter("data", "=", "\\x00ff")]), ..Default::default() }).await;
        assert_eq!(found.total_rows, 1);
    }

    #[tokio::test]
    async fn typeless_keys_find_their_own_rows() {
        let f = TempDb::with(
            "CREATE TABLE u (id PRIMARY KEY, name);
             INSERT INTO u VALUES (1, 'int one'), ('1', 'text one'), (2, 'two'), (9007199254740993, 'big'), (2.5, 'real');",
        );
        let d = open(&f).await;
        let read = page(&d, "u", TableDataParams { sort: Some(vec![sort("name", "asc")]), ..Default::default() }).await;
        // As the grid shows them: big, int one, real, text one, two
        assert_eq!(first_column(&read), vec![json!("9007199254740993"), json!(1), json!(2.5), json!("1"), json!(2)]);

        // Each key, as the grid holds it, finds exactly its own row
        let renamed: Vec<RowChange> = read
            .rows
            .iter()
            .map(|row| change(json!({ "id": row[0] }), json!({ "name": format!("{} edited", row[1].as_str().unwrap()) })))
            .collect();
        assert_eq!(d.data().save_rows("main", "u", renamed).await.unwrap(), 5);
        let stored = query(&d, "SELECT id, typeof(id), name FROM u ORDER BY name").await;
        assert_eq!(
            stored,
            vec![
                vec![json!("9007199254740993"), json!("integer"), json!("big edited")],
                vec![json!(1), json!("integer"), json!("int one edited")],
                vec![json!(2.5), json!("real"), json!("real edited")],
                vec![json!("1"), json!("text"), json!("text one edited")],
                vec![json!(2), json!("integer"), json!("two edited")],
            ]
        );

        // The text '1' goes, the integer 1 stays; then the integer and the big one go
        assert_eq!(d.data().delete_rows("main", "u", vec![values(json!({ "id": "1" }))]).await.unwrap(), 1);
        let keys = vec![values(json!({ "id": 1 })), values(json!({ "id": "9007199254740993" }))];
        assert_eq!(d.data().delete_rows("main", "u", keys).await.unwrap(), 2);
        assert_eq!(query(&d, "SELECT id, typeof(id) FROM u ORDER BY id").await, vec![vec![json!(2), json!("integer")], vec![json!(2.5), json!("real")]]);
    }

    #[tokio::test]
    async fn real_and_blob_keys_find_their_rows() {
        let f = TempDb::with(
            "CREATE TABLE rk (k REAL PRIMARY KEY, v TEXT); INSERT INTO rk VALUES (1.5, 'a'), (1e999, 'b'), (-1e999, 'c');
             CREATE TABLE ids (id BLOB PRIMARY KEY, v TEXT);
             INSERT INTO ids VALUES (x'00112233445566778899aabbccddeeff', 'a'), (x'ffeeddccbbaa99887766554433221100', 'b');
             CREATE TABLE amb (id PRIMARY KEY, v); INSERT INTO amb VALUES (x'00ff', 'blob'), ('\\x00ff', 'text');",
        );
        let d = open(&f).await;

        let reals = page(&d, "rk", TableDataParams { sort: Some(vec![sort("v", "asc")]), ..Default::default() }).await;
        assert_eq!(first_column(&reals), vec![json!(1.5), json!("Infinity"), json!("-Infinity")]);
        let edits = reals.rows.iter().map(|r| change(json!({ "k": r[0] }), json!({ "v": "edited" }))).collect();
        assert_eq!(d.data().save_rows("main", "rk", edits).await.unwrap(), 3);
        assert_eq!(d.data().delete_rows("main", "rk", vec![values(json!({ "k": "Infinity" }))]).await.unwrap(), 1);
        assert_eq!(query(&d, "SELECT k, v FROM rk ORDER BY k").await, vec![vec![json!("-Infinity"), json!("edited")], vec![json!(1.5), json!("edited")]]);

        let uuids = page(&d, "ids", TableDataParams { sort: Some(vec![sort("v", "asc")]), ..Default::default() }).await;
        assert_eq!(uuids.rows[0][0], json!("\\x00112233445566778899aabbccddeeff"));
        let edit = change(json!({ "id": uuids.rows[0][0] }), json!({ "v": "A" }));
        assert_eq!(d.data().save_rows("main", "ids", vec![edit]).await.unwrap(), 1);
        assert_eq!(d.data().delete_rows("main", "ids", vec![values(json!({ "id": uuids.rows[1][0] }))]).await.unwrap(), 1);
        let stored = query(&d, "SELECT hex(id), typeof(id), v FROM ids").await;
        assert_eq!(stored, vec![vec![json!("00112233445566778899AABBCCDDEEFF"), json!("blob"), json!("A")]]);

        // A blob and the text that looks like it: the key can't tell them apart, so nothing changes
        let edit = change(json!({ "id": "\\x00ff" }), json!({ "v": "which?" }));
        let err = d.data().save_rows("main", "amb", vec![edit]).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. The row: its key matched 2 rows");
        let err = d.data().delete_rows("main", "amb", vec![values(json!({ "id": "\\x00ff" }))]).await.unwrap_err();
        assert_eq!(err, "Nothing was deleted: the key matched 2 rows");
        assert_eq!(d.data().count_rows("main", "amb").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn typeless_columns_store_numbers_as_numbers() {
        let f = TempDb::with("CREATE TABLE w (id INTEGER PRIMARY KEY, qty, note TEXT); INSERT INTO w VALUES (1, 5, 'x'), (2, 7, 'y');");
        let d = open(&f).await;
        let edits = vec![change(json!({ "id": 1 }), json!({ "qty": "6" })), change(json!({ "id": 2 }), json!({ "qty": "007", "note": "8" }))];
        assert_eq!(d.data().save_rows("main", "w", edits).await.unwrap(), 2);
        d.data().insert_row("main", "w", values(json!({ "id": 3, "qty": "2.5" }))).await.unwrap();
        d.data().insert_row("main", "w", values(json!({ "id": 4, "qty": "-12" }))).await.unwrap();
        d.data().insert_row("main", "w", values(json!({ "id": 5, "qty": true }))).await.unwrap();
        let rows = vec![vec![Some("6".to_string()), Some("1e5".to_string())], vec![Some("7".to_string()), Some("abc".to_string())]];
        d.data().import_rows("main", "w", vec!["id".into(), "qty".into()], rows).await.unwrap();
        let stored = query(&d, "SELECT qty, typeof(qty), typeof(note) FROM w ORDER BY id").await;
        assert_eq!(
            stored,
            vec![
                vec![json!(6), json!("integer"), json!("text")],
                vec![json!("007"), json!("text"), json!("text")],
                vec![json!(2.5), json!("real"), json!("null")],
                vec![json!(-12), json!("integer"), json!("null")],
                vec![json!(1), json!("integer"), json!("null")],
                vec![json!(100000.0), json!("real"), json!("null")],
                vec![json!("abc"), json!("text"), json!("null")],
            ]
        );
        let bigger = page(&d, "w", TableDataParams { filters: Some(vec![filter("qty", ">", "5")]), ..Default::default() }).await;
        assert_eq!(first_column(&bigger), vec![json!(1), json!(2), json!(6), json!(7)], "6, 1e5, and the texts, which sort after every number");
    }

    #[tokio::test]
    async fn blobs_written_back_keep_their_bytes() {
        let f = TempDb::with("CREATE TABLE bl (id INTEGER PRIMARY KEY, data BLOB, raw); INSERT INTO bl VALUES (1, x'00ff', x'deadbeef');");
        let d = open(&f).await;
        let read = page(&d, "bl", TableDataParams::default()).await;
        assert_eq!(read.rows[0], vec![json!(1), json!("\\x00ff"), json!("\\xdeadbeef")]);

        // Saved unchanged, cloned as a new row, and imported
        let edit = change(json!({ "id": 1 }), json!({ "data": read.rows[0][1], "raw": read.rows[0][2] }));
        assert_eq!(d.data().save_rows("main", "bl", vec![edit]).await.unwrap(), 1);
        let clone = values(json!({ "data": read.rows[0][1], "raw": read.rows[0][2] }));
        assert_eq!(d.data().insert_row("main", "bl", clone).await.unwrap(), 1);
        let rows = vec![vec![Some("\\x00FF".to_string()), Some("\\xdeadbeef".to_string())], vec![Some("\\x0".to_string()), Some("\\xzz".to_string())]];
        assert_eq!(d.data().import_rows("main", "bl", vec!["data".into(), "raw".into()], rows).await.unwrap(), 2);

        let stored = query(&d, "SELECT hex(data), typeof(data), hex(raw), typeof(raw) FROM bl ORDER BY id").await;
        let blob = vec![json!("00FF"), json!("blob"), json!("DEADBEEF"), json!("blob")];
        assert_eq!(stored[..3], [blob.clone(), blob.clone(), blob]);
        assert_eq!(stored[3], vec![json!("5C7830"), json!("text"), json!("5C787A7A"), json!("text")], "not hex digits in pairs: text");
    }

    #[tokio::test]
    async fn filters_on_columns_without_a_type_compare_numbers_as_numbers() {
        let f = TempDb::with(
            "CREATE TABLE nums (id INTEGER PRIMARY KEY, x, b BLOB, g AS (x + 0));
             INSERT INTO nums (x, b) VALUES (1, 1), (2, 2), (9, 9), (10, 10);
             CREATE TABLE items (grp TEXT);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 22)
             INSERT INTO items SELECT CASE WHEN i = 1 THEN 'a' WHEN i <= 3 THEN 'b' WHEN i <= 12 THEN 'c' ELSE 'd' END FROM n;
             CREATE VIEW counts AS SELECT grp, count(*) AS n FROM items GROUP BY grp;",
        );
        let d = open(&f).await;
        let total = |table: &'static str, column: &'static str, operator: &'static str, value: &'static str| {
            let d = d.clone();
            async move {
                let params = TableDataParams { filters: Some(vec![filter(column, operator, value)]), ..Default::default() };
                page(&d, table, params).await.total_rows
            }
        };
        for (table, column) in [("nums", "x"), ("nums", "b"), ("nums", "g"), ("counts", "n")] {
            let found = (
                total(table, column, ">", "9").await,
                total(table, column, "=", "1").await,
                total(table, column, "!=", "1").await,
                total(table, column, "in", "1, 2").await,
                total(table, column, "not in", "1, 2").await,
                total(table, column, "like", "1%").await,
            );
            assert_eq!(found, (1, 1, 3, 2, 2, 2), "{table}.{column}");
        }
    }

    #[tokio::test]
    async fn a_later_delete_batch_that_fails_keeps_the_earlier_ones() {
        let f = TempDb::with(
            "CREATE TABLE parents (id INTEGER PRIMARY KEY);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300) INSERT INTO parents SELECT i FROM n;
             CREATE TABLE children (parent_id INTEGER REFERENCES parents (id)); INSERT INTO children VALUES (300);",
        );
        let d = open(&f).await;
        let keys: Vec<JsonMap> = (1..=300).map(|id: i64| values(json!({ "id": id }))).collect();
        let err = d.data().delete_rows("main", "parents", keys).await.unwrap_err();
        assert_eq!(err, "FOREIGN KEY constraint failed");
        assert_eq!(d.data().count_rows("main", "parents").await.unwrap(), 300, "the first 250 are back");
    }

    #[tokio::test]
    async fn row_keys_follow_the_table_kind() {
        let f = TempDb::with(
            "CREATE TABLE plain (body TEXT); INSERT INTO plain VALUES ('a'), ('b');
             CREATE TABLE named (rowid TEXT, body TEXT); INSERT INTO named VALUES ('r1', 'a'), ('r2', 'b');
             CREATE TABLE taken (rowid TEXT, _rowid_ TEXT, oid TEXT); INSERT INTO taken VALUES ('x', 'y', 'z');
             CREATE TABLE codes (code TEXT PRIMARY KEY, n INTEGER) WITHOUT ROWID; INSERT INTO codes VALUES ('a', 1), ('b', 2);",
        );
        let d = open(&f).await;

        // No primary key: the rowid comes first and is the key
        let plain = page(&d, "plain", TableDataParams::default()).await;
        assert_eq!(column_names(&plain), vec!["rowid", "body"]);
        assert_eq!(plain.columns[0].data_type, "bigint");
        assert_eq!(plain.rows, vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]]);
        let edit = change(json!({ "rowid": 2 }), json!({ "body": "B" }));
        assert_eq!(d.data().save_rows("main", "plain", vec![edit]).await.unwrap(), 1);
        assert_eq!(d.data().delete_rows("main", "plain", vec![values(json!({ "rowid": 1 }))]).await.unwrap(), 1);
        assert_eq!(query(&d, "SELECT rowid, body FROM plain").await, vec![vec![json!(2), json!("B")]]);
        let sorted = page(&d, "plain", TableDataParams { sort: Some(vec![sort("rowid", "desc")]), ..Default::default() }).await;
        assert_eq!(sorted.rows[0][0], json!(2));

        // A real column called rowid: the key is _rowid_, and the real column stays editable
        let named = page(&d, "named", TableDataParams::default()).await;
        assert_eq!(column_names(&named), vec!["_rowid_", "rowid", "body"]);
        assert_eq!(named.rows[1], vec![json!(2), json!("r2"), json!("b")]);
        let edit = change(json!({ "_rowid_": 2 }), json!({ "rowid": "renamed" }));
        assert_eq!(d.data().save_rows("main", "named", vec![edit]).await.unwrap(), 1);
        assert_eq!(query(&d, "SELECT rowid FROM named ORDER BY _rowid_").await, vec![vec![json!("r1")], vec![json!("renamed")]]);

        // Every name of the rowid taken: no key, so the grid can't edit it
        let taken = page(&d, "taken", TableDataParams::default()).await;
        assert_eq!(column_names(&taken), vec!["rowid", "_rowid_", "oid"]);
        assert_eq!(taken.rows, vec![vec![json!("x"), json!("y"), json!("z")]]);
        let columns = d.columns("main", "taken").await.unwrap();
        assert!(columns.iter().all(|c| !c.is_primary_key));

        // WITHOUT ROWID: the primary key
        let codes = page(&d, "codes", TableDataParams::default()).await;
        assert_eq!(column_names(&codes), vec!["code", "n"]);
        let edit = change(json!({ "code": "b" }), json!({ "n": 20 }));
        assert_eq!(d.data().save_rows("main", "codes", vec![edit]).await.unwrap(), 1);
        assert_eq!(query(&d, "SELECT n FROM codes ORDER BY code").await, vec![vec![json!(1)], vec![json!(20)]]);
    }

    #[tokio::test]
    async fn saving_is_all_or_nothing() {
        let f = TempDb::with(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL); INSERT INTO t VALUES (1, 'a'), (2, 'b');
             CREATE TABLE dup (tag TEXT); INSERT INTO dup VALUES ('x'), ('x');",
        );
        let d = open(&f).await;
        let names = || query(&d, "SELECT name FROM t ORDER BY id");

        // The second row breaks NOT NULL: the first isn't saved either
        let changes = vec![change(json!({ "id": 1 }), json!({ "name": "A" })), change(json!({ "id": 2 }), json!({ "name": null }))];
        let err = d.data().save_rows("main", "t", changes).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. Row 2 of 2: NOT NULL constraint failed: t.name");
        assert_eq!(names().await, vec![vec![json!("a")], vec![json!("b")]]);

        let missing = vec![change(json!({ "id": 9 }), json!({ "name": "z" }))];
        let err = d.data().save_rows("main", "t", missing).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. The row: it wasn't found, so it was changed or deleted since it was loaded");

        let several = vec![change(json!({ "tag": "x" }), json!({ "tag": "y" }))];
        let err = d.data().save_rows("main", "dup", several).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. The row: its key matched 2 rows");
        assert_eq!(query(&d, "SELECT tag FROM dup").await, vec![vec![json!("x")], vec![json!("x")]]);

        let unknown = vec![change(json!({ "id": 1 }), json!({ "name": "A" })), change(json!({ "id": 2 }), json!({ "nope": 1 }))];
        let err = d.data().save_rows("main", "t", unknown).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. Row 2 of 2: Invalid column: nope");
        assert_eq!(names().await, vec![vec![json!("a")], vec![json!("b")]]);

        // The connection is usable after the failures, and a good save saves every row
        let good = vec![change(json!({ "id": 1 }), json!({ "name": "A" })), change(json!({ "id": 2 }), json!({ "name": "B" }))];
        assert_eq!(d.data().save_rows("main", "t", good).await.unwrap(), 2);
        assert_eq!(names().await, vec![vec![json!("A")], vec![json!("B")]]);
    }

    #[tokio::test]
    async fn deletes_are_all_or_nothing() {
        let f = TempDb::with(
            "CREATE TABLE parents (id INTEGER PRIMARY KEY);
             CREATE TABLE children (parent_id INTEGER REFERENCES parents (id));
             INSERT INTO parents VALUES (1), (2), (3);
             INSERT INTO children VALUES (2);
             CREATE TABLE many (id INTEGER PRIMARY KEY);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1500) INSERT INTO many SELECT i FROM n;",
        );
        let d = open(&f).await;
        let key = |id: i64| values(json!({ "id": id }));

        // Row 2 is referenced, so deleting 1 and 2 together fails and keeps both
        let err = d.data().delete_rows("main", "parents", vec![key(1), key(2)]).await.unwrap_err();
        assert_eq!(err, "FOREIGN KEY constraint failed");
        assert_eq!(d.data().count_rows("main", "parents").await.unwrap(), 3);

        assert_eq!(d.data().delete_rows("main", "parents", vec![key(1), key(3), key(99)]).await.unwrap(), 2);
        assert_eq!(query(&d, "SELECT id FROM parents").await, vec![vec![json!(2)]]);

        // Many keys at once, in one go
        let keys: Vec<JsonMap> = (1..=1200).map(key).collect();
        assert_eq!(d.data().delete_rows("main", "many", keys).await.unwrap(), 1200);
        assert_eq!(d.data().count_rows("main", "many").await.unwrap(), 300);

        let err = d.data().delete_rows("main", "parents", vec![]).await.unwrap_err();
        assert_eq!(err, "primaryKeys is required");
        let err = d.data().delete_rows("main", "parents", vec![values(json!({ "nope": 1 }))]).await.unwrap_err();
        assert_eq!(err, "Invalid column: nope");
    }

    #[tokio::test]
    async fn imports_are_all_or_nothing() {
        let f = TempDb::with("CREATE TABLE imported (id INTEGER PRIMARY KEY, name TEXT NOT NULL, score REAL)");
        let d = open(&f).await;
        let columns: Vec<String> = vec!["id".into(), "name".into(), "score".into()];
        let row = |id: i64, name: &str| vec![Some(id.to_string()), Some(name.to_string()), Some("1.5".to_string())];

        let rows: Vec<_> = (1..=1500).map(|i| row(i, &format!("person {i}"))).collect();
        assert_eq!(d.data().import_rows("main", "imported", columns.clone(), rows).await.unwrap(), 1500);
        let kinds = query(&d, "SELECT DISTINCT typeof(id), typeof(score) FROM imported").await;
        assert_eq!(kinds, vec![vec![json!("integer"), json!("real")]], "numeric text takes the column's type");

        // A duplicate key in the second batch rolls back the first one too
        let mut bad: Vec<_> = (2001..=3200).map(|i| row(i, "x")).collect();
        bad.push(row(1, "duplicate"));
        let err = d.data().import_rows("main", "imported", columns.clone(), bad).await.unwrap_err();
        assert_eq!(err, "Nothing was imported. A row between 1001 and 1201 failed: UNIQUE constraint failed: imported.id");
        assert_eq!(d.data().count_rows("main", "imported").await.unwrap(), 1500);

        let missing_name = vec![vec![Some("9999".to_string()), None, None]];
        let err = d.data().import_rows("main", "imported", columns.clone(), missing_name).await.unwrap_err();
        assert_eq!(err, "Nothing was imported. A row between 1 and 1 failed: NOT NULL constraint failed: imported.name");
        let err = d.data().import_rows("main", "imported", vec!["nope".into()], vec![vec![None]]).await.unwrap_err();
        assert_eq!(err, "Invalid column: nope");
        assert_eq!(d.data().count_rows("main", "imported").await.unwrap(), 1500);
    }

    #[tokio::test]
    async fn foreign_keys_are_enforced() {
        let f = TempDb::with(
            "CREATE TABLE parents (id INTEGER PRIMARY KEY);
             CREATE TABLE children (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parents (id));
             INSERT INTO parents VALUES (1); INSERT INTO children VALUES (1, 1);",
        );
        let d = open(&f).await;
        let err = d.data().insert_row("main", "children", values(json!({ "parent_id": 99 }))).await.unwrap_err();
        assert_eq!(err, "FOREIGN KEY constraint failed");
        let edit = change(json!({ "id": 1 }), json!({ "parent_id": 99 }));
        let err = d.data().save_rows("main", "children", vec![edit]).await.unwrap_err();
        assert_eq!(err, "Nothing was saved. The row: FOREIGN KEY constraint failed");
        let rows = vec![vec![Some("2".to_string()), Some("1".to_string())], vec![Some("3".to_string()), Some("99".to_string())]];
        let err = d.data().import_rows("main", "children", vec!["id".into(), "parent_id".into()], rows).await.unwrap_err();
        assert_eq!(err, "Nothing was imported. A row between 1 and 2 failed: FOREIGN KEY constraint failed");
        assert_eq!(query(&d, "SELECT id, parent_id FROM children").await, vec![vec![json!(1), json!(1)]]);
    }

    #[tokio::test]
    async fn inserts_use_defaults_and_explicit_nulls() {
        let f = TempDb::with("CREATE TABLE ins (id INTEGER PRIMARY KEY, name TEXT NOT NULL, note TEXT DEFAULT 'x', n INTEGER)");
        let d = open(&f).await;
        assert_eq!(d.data().insert_row("main", "ins", values(json!({ "name": "ana", "n": "42" }))).await.unwrap(), 1);
        assert_eq!(d.data().insert_row("main", "ins", values(json!({ "name": "bia", "note": null }))).await.unwrap(), 1);
        let rows = query(&d, "SELECT id, name, note, n, typeof(n) FROM ins ORDER BY id").await;
        assert_eq!(rows[0], vec![json!(1), json!("ana"), json!("x"), json!(42), json!("integer")]);
        assert_eq!(rows[1], vec![json!(2), json!("bia"), JsonValue::Null, JsonValue::Null, json!("null")]);

        let err = d.data().insert_row("main", "ins", values(json!({}))).await.unwrap_err();
        assert_eq!(err, "NOT NULL constraint failed: ins.name");
        let err = d.data().insert_row("main", "ins", values(json!({ "nope": 1 }))).await.unwrap_err();
        assert_eq!(err, "Invalid column: nope");
        assert_eq!(d.data().count_rows("main", "ins").await.unwrap(), 2);
    }

    #[test]
    fn a_save_does_not_wait_for_a_busy_editor() {
        without_hanging(async {
            let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO t VALUES (1, 'a');");
            let opts = OpenOptions { busy_timeout: std::time::Duration::from_millis(200), ..Default::default() };
            let d = SqliteDriver::open(f.path(), opts).await.unwrap();
            d.editor().run(|c| c.execute_batch("BEGIN; INSERT INTO t VALUES (2, 'b');").map_err(|e| e.to_string())).await.unwrap();
            // The editor goes on with a statement that never ends, inside its transaction
            let mut running = {
                let d = d.clone();
                tokio::spawn(async move {
                    let endless = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) SELECT count(*) FROM n";
                    d.editor().run(move |c| c.query_row(endless, [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())).await
                })
            };
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let started = std::time::Instant::now();
            let save = d.data().save_rows("main", "t", vec![change(json!({ "id": 1 }), json!({ "name": "A" }))]);
            let saved = tokio::time::timeout(std::time::Duration::from_secs(3), save).await;
            let elapsed = started.elapsed();
            // Until it stops: an interrupt that comes before the statement starts is lost
            let interrupt = || std::future::ready(d.editor().interrupt());
            let (stopped, _) = stop_until_done(&mut running, std::time::Duration::from_millis(20), std::time::Duration::from_secs(5), interrupt).await;
            assert!(stopped.is_err(), "the endless statement was stopped");

            let err = saved.expect("the save must not wait for the editor's statement").unwrap_err();
            assert_eq!(err, "A transaction in the query editor is holding the database: commit or roll it back, then try again.");
            assert!(elapsed < std::time::Duration::from_secs(2), "{elapsed:?}");
        });
    }
}
