//! What a SQLite file holds: the explorer, columns, structure, object definitions and
//! autocomplete. A "schema" is a database of the connection: main, temp or an attached one.

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};

use super::errors::map_statement_error;
use super::values::standard_type;
use super::SqliteDriver;
use crate::db::model::{
    ColumnInfo, CompletionColumn, CompletionRelation, CompletionSchema, ConstraintInfo, ExplorerObject, ExplorerSchema,
    ExplorerTree, ExtensionInfo, ForeignKeyInfo, ForeignKeyRef, FunctionDetail, IndexInfo, ObjectDefinition,
    ReferencedBy, TableMeta, TableStructure, TriggerInfo,
};
use crate::db::sql::Dialect;
use crate::db::{unsupported, Catalog, Engine};

#[async_trait]
impl Catalog for SqliteDriver {
    async fn explorer(&self) -> Result<ExplorerTree, String> {
        self.tables().run(|c| explorer(c).map_err(db_error)).await
    }

    async fn columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.tables().run(move |c| table_columns(c, &schema, &table).map_err(db_error)).await
    }

    async fn relation_kind(&self, schema: &str, name: &str) -> Result<Option<String>, String> {
        let (schema, name) = (schema.to_owned(), name.to_owned());
        self.tables().run(move |c| relation_kind(c, &schema, &name).map_err(db_error)).await
    }

    async fn table_structure(&self, schema: &str, table: &str) -> Result<TableStructure, String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.tables().run(move |c| table_structure(c, &schema, &table)).await
    }

    async fn completion_schema(&self) -> Result<CompletionSchema, String> {
        self.tables().run(|c| completion_schema(c).map_err(db_error)).await
    }

    async fn object_definition(&self, schema: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String> {
        let (schema, name, kind) = (schema.to_owned(), name.to_owned(), kind.to_owned());
        self.tables().run(move |c| object_definition(c, &schema, &name, &kind)).await
    }

    async fn function_detail(&self, _: &str, _: &str, _: &str) -> Result<FunctionDetail, String> {
        Err(unsupported(Engine::Sqlite, "Functions"))
    }

    async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String> {
        Err(unsupported(Engine::Sqlite, "Extensions"))
    }
}

/// A statement error in the app's words. The catalog only reads on the tables'
/// connection; a lock that stops it is reported as another program's.
fn db_error(e: rusqlite::Error) -> String {
    map_statement_error(&e, false)
}

fn quote(name: &str) -> String {
    Dialect::Sqlite.quote_ident(name)
}

/// The databases of the connection, in its order: main, temp, then the attached ones.
fn databases(c: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare("PRAGMA database_list")?;
    let names = stmt.query_map([], |r| r.get::<_, String>("name"))?;
    names.collect()
}

/// One row of a database's `sqlite_master`.
struct Object {
    kind: String,
    name: String,
    /// The table an index or trigger belongs to (a table's or view's own name otherwise).
    table: String,
}

/// The user's tables, views, indexes and triggers of `db`, by kind and name: no internal
/// `sqlite_` objects and no automatic indexes (those have no SQL).
fn objects(c: &Connection, db: &str) -> rusqlite::Result<Vec<Object>> {
    let sql = format!(
        "SELECT type, name, tbl_name FROM {}.sqlite_master \
         WHERE type IN ('table', 'view', 'index', 'trigger') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' \
           AND sql IS NOT NULL \
         ORDER BY type, name",
        quote(db)
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok(Object { kind: r.get(0)?, name: r.get(1)?, table: r.get(2)? }))?;
    rows.collect()
}

/// What `PRAGMA table_list` says about a table or view.
struct TableEntry {
    /// table, view, virtual or shadow.
    kind: String,
    without_rowid: bool,
}

fn table_entry(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Option<TableEntry>> {
    let sql = format!("PRAGMA {}.table_list({})", quote(db), quote(table));
    c.query_row(&sql, [], |r| {
        Ok(TableEntry { kind: r.get("type")?, without_rowid: r.get::<_, i64>("wr")? != 0 })
    })
    .optional()
}

/// Names of the `WITHOUT ROWID` tables of `db`.
fn without_rowid_tables(c: &Connection, db: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare(&format!("PRAGMA {}.table_list", quote(db)))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>("name")?, r.get::<_, i64>("wr")?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|(_, wr)| *wr != 0).map(|(name, _)| name).collect())
}

fn explorer(c: &Connection) -> rusqlite::Result<ExplorerTree> {
    let mut schemas = Vec::new();
    for db in databases(c)? {
        let found = objects(c, &db)?;
        if db == "temp" && found.is_empty() {
            continue;
        }
        let without_rowid = without_rowid_tables(c, &db)?;
        let objects = found
            .into_iter()
            .map(|o| {
                let detail = match o.kind.as_str() {
                    "index" | "trigger" => o.table,
                    "table" if without_rowid.contains(&o.name) => "without rowid".to_string(),
                    _ => String::new(),
                };
                ExplorerObject { name: o.name, kind: o.kind, detail, partitions: Vec::new() }
            })
            .collect();
        schemas.push(ExplorerSchema { name: db, objects });
    }
    Ok(ExplorerTree { schemas, default_schema: Some("main".into()) })
}

/// One row of `PRAGMA table_xinfo`.
struct RawColumn {
    cid: i64,
    name: String,
    declared: String,
    not_null: bool,
    default: Option<String>,
    /// Position in the primary key, from 1; 0 when the column isn't part of it.
    pk: i64,
    /// 1 hidden (virtual tables), 2 virtual generated, 3 stored generated.
    hidden: i64,
}

fn raw_columns(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<RawColumn>> {
    let mut stmt = c.prepare(&format!("PRAGMA {}.table_xinfo({})", quote(db), quote(table)))?;
    let rows = stmt.query_map([], |r| {
        Ok(RawColumn {
            cid: r.get("cid")?,
            name: r.get("name")?,
            declared: r.get::<_, Option<String>>("type")?.unwrap_or_default(),
            not_null: r.get::<_, i64>("notnull")? != 0,
            default: r.get("dflt_value")?,
            pk: r.get("pk")?,
            hidden: r.get("hidden")?,
        })
    })?;
    rows.collect()
}

/// The primary key's columns, in key order.
fn primary_key(columns: &[RawColumn]) -> Vec<String> {
    let mut key: Vec<&RawColumn> = columns.iter().filter(|c| c.pk > 0).collect();
    key.sort_by_key(|c| c.pk);
    key.into_iter().map(|c| c.name.clone()).collect()
}

/// A foreign key of a table, put together from its `PRAGMA foreign_key_list` rows.
struct RawForeignKey {
    table: String,
    from: Vec<String>,
    to: Vec<String>,
    on_update: String,
    on_delete: String,
}

impl RawForeignKey {
    /// SQLite keeps no names for foreign keys, so the key is named after what it links.
    fn name(&self) -> String {
        format!("{} → {}", self.from.join(", "), self.table)
    }
}

/// The table's foreign keys in the order they were declared. A key declared without
/// columns points at the referenced table's primary key.
fn foreign_keys(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<RawForeignKey>> {
    let mut stmt = c.prepare(&format!("PRAGMA {}.foreign_key_list({})", quote(db), quote(table)))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>("id")?,
            r.get::<_, String>("table")?,
            r.get::<_, String>("from")?,
            r.get::<_, Option<String>>("to")?,
            r.get::<_, String>("on_update")?,
            r.get::<_, String>("on_delete")?,
        ))
    })?;
    let mut keys: Vec<(i64, RawForeignKey, bool)> = Vec::new();
    for row in rows {
        let (id, parent, from, to, on_update, on_delete) = row?;
        let implicit = to.is_none();
        match keys.iter_mut().find(|(k, ..)| *k == id) {
            Some((_, key, _)) => {
                key.from.push(from);
                key.to.extend(to);
            }
            None => keys.push((
                id,
                RawForeignKey { table: parent, from: vec![from], to: to.into_iter().collect(), on_update, on_delete },
                implicit,
            )),
        }
    }
    // SQLite numbers the keys from the last one declared
    keys.sort_by_key(|(id, ..)| std::cmp::Reverse(*id));
    keys.into_iter()
        .map(|(_, mut key, implicit)| {
            if implicit {
                key.to = primary_key(&raw_columns(c, db, &key.table)?);
            }
            Ok(key)
        })
        .collect()
}

/// The names the rowid of a table answers to, in the order the synthetic column takes them.
const ROWID_NAMES: [&str; 3] = ["rowid", "_rowid_", "oid"];

fn table_columns(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<ColumnInfo>> {
    let raw = raw_columns(c, db, table)?;
    let keys = foreign_keys(c, db, table)?;
    let entry = table_entry(c, db, table)?;
    let without_rowid = entry.as_ref().is_some_and(|e| e.without_rowid);

    let mut columns: Vec<ColumnInfo> = raw
        .iter()
        .filter(|r| r.hidden != 1)
        .map(|r| ColumnInfo {
            name: r.name.clone(),
            data_type: standard_type(&r.declared).to_string(),
            type_name: r.declared.clone(),
            // The key of a WITHOUT ROWID table is never NULL, whatever was declared
            is_nullable: !r.not_null && !(without_rowid && r.pk > 0),
            default_value: r.default.clone(),
            ordinal_position: (r.cid + 1) as i32,
            is_primary_key: r.pk > 0,
            is_generated: r.hidden == 2 || r.hidden == 3,
            comment: None,
            enum_values: None,
            foreign_key: keys.iter().find(|k| k.from.contains(&r.name)).map(|k| ForeignKeyRef {
                constraint: k.name(),
                schema: db.to_string(),
                table: k.table.clone(),
                columns: k.from.clone(),
                ref_columns: k.to.clone(),
            }),
        })
        .collect();

    // A table without a key is edited by its rowid, under the first name no column took
    let has_rowid = entry.as_ref().is_some_and(|e| e.kind != "view" && !e.without_rowid);
    if has_rowid && raw.iter().all(|r| r.pk == 0) {
        let free = ROWID_NAMES.into_iter().find(|n| !raw.iter().any(|r| r.name.eq_ignore_ascii_case(n)));
        if let Some(name) = free {
            columns.insert(
                0,
                ColumnInfo {
                    name: name.to_string(),
                    data_type: standard_type("INTEGER").to_string(),
                    type_name: "INTEGER".into(),
                    is_nullable: false,
                    default_value: None,
                    ordinal_position: 0,
                    is_primary_key: true,
                    is_generated: true,
                    comment: None,
                    enum_values: None,
                    foreign_key: None,
                },
            );
        }
    }
    Ok(columns)
}

/// `table`, `view`, `index` or `trigger`; None when `db` has nothing by that name (or
/// there's no such database).
fn relation_kind(c: &Connection, db: &str, name: &str) -> rusqlite::Result<Option<String>> {
    if !databases(c)?.iter().any(|d| d.eq_ignore_ascii_case(db)) {
        return Ok(None);
    }
    let sql = format!(
        "SELECT type FROM {}.sqlite_master WHERE name = ?1 COLLATE NOCASE \
         AND type IN ('table', 'view', 'index', 'trigger')",
        quote(db)
    );
    c.query_row(&sql, [name], |r| r.get(0)).optional()
}

/// The `sql` SQLite keeps for an object of `db`, if it keeps any.
fn stored_sql(c: &Connection, db: &str, kinds: &str, name: &str) -> rusqlite::Result<Option<(String, Option<String>)>> {
    let sql = format!(
        "SELECT tbl_name, sql FROM {}.sqlite_master WHERE type IN ({kinds}) AND name = ?1 COLLATE NOCASE",
        quote(db)
    );
    c.query_row(&sql, [name], |r| Ok((r.get(0)?, r.get(1)?))).optional()
}

fn object_definition(c: &Connection, db: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String> {
    if !matches!(kind, "table" | "view" | "index" | "trigger") {
        return Err(format!("Unknown object kind \"{kind}\""));
    }
    let (table, sql) = stored_sql(c, db, &format!("'{kind}'"), name)
        .map_err(db_error)?
        .ok_or_else(|| format!("{db}.{name} doesn't exist"))?;
    let facts = if matches!(kind, "index" | "trigger") { vec![("Table".to_string(), table)] } else { Vec::new() };
    Ok(ObjectDefinition { kind: kind.into(), facts, comment: None, ddl: sql.map(|s| format!("{s};")).unwrap_or_default() })
}

/// A section that failed becomes a warning and an empty value.
fn section<T: Default>(warnings: &mut Vec<String>, name: &str, result: rusqlite::Result<T>) -> T {
    result.unwrap_or_else(|e| {
        let warning = format!("Couldn't read {name}: {}", db_error(e));
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
        T::default()
    })
}

fn table_structure(c: &Connection, db: &str, table: &str) -> Result<TableStructure, String> {
    let entry = table_entry(c, db, table).map_err(db_error)?.ok_or_else(|| format!("{db}.{table} doesn't exist"))?;
    let mut warnings = Vec::new();
    let raw = section(&mut warnings, "the columns", raw_columns(c, db, table));
    let columns = section(&mut warnings, "the columns", table_columns(c, db, table));
    let keys = section(&mut warnings, "the foreign keys", foreign_keys(c, db, table));
    let indexes = section(&mut warnings, "the indexes", table_indexes(c, db, table));
    let referenced_by = section(&mut warnings, "the referencing tables", referencing_tables(c, db, table));
    let triggers = section(&mut warnings, "the triggers", table_triggers(c, db, table));
    let own_sql = section(&mut warnings, "the definition", stored_sql(c, db, "'table', 'view'", table));

    let meta = TableMeta {
        kind: match entry.kind.as_str() {
            "view" => "view",
            "virtual" => "virtual table",
            _ => "table",
        }
        .into(),
        ..TableMeta::default()
    };

    let names = |columns: &[String]| columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
    let mut constraints = Vec::new();
    let key = primary_key(&raw);
    if !key.is_empty() {
        constraints.push(ConstraintInfo {
            name: indexes.iter().find(|i| i.is_primary).map(|i| i.name.clone()).unwrap_or_default(),
            kind: "primary key".into(),
            definition: format!("PRIMARY KEY ({})", names(&key)),
        });
    }
    for index in indexes.iter().filter(|i| i.backs_constraint && !i.is_primary) {
        constraints.push(ConstraintInfo {
            name: index.name.clone(),
            kind: "unique".into(),
            definition: format!("UNIQUE ({})", names(&index.columns)),
        });
    }
    for k in &keys {
        let mut definition = format!("FOREIGN KEY ({}) REFERENCES {} ({})", names(&k.from), quote(&k.table), names(&k.to));
        if k.on_update != "NO ACTION" {
            definition += &format!(" ON UPDATE {}", k.on_update);
        }
        if k.on_delete != "NO ACTION" {
            definition += &format!(" ON DELETE {}", k.on_delete);
        }
        constraints.push(ConstraintInfo { name: k.name(), kind: "foreign key".into(), definition });
    }

    let foreign_keys = keys
        .iter()
        .map(|k| ForeignKeyInfo {
            name: k.name(),
            columns: k.from.clone(),
            ref_schema: db.to_string(),
            ref_table: k.table.clone(),
            ref_columns: k.to.clone(),
            on_update: k.on_update.clone(),
            on_delete: k.on_delete.clone(),
        })
        .collect();

    // The SQL SQLite kept, then the indexes and triggers that aren't part of it
    let mut ddl = own_sql.and_then(|(_, sql)| sql).map(|sql| format!("{sql};")).unwrap_or_default();
    let extra_indexes: Vec<&IndexInfo> = indexes.iter().filter(|i| !i.backs_constraint).collect();
    if !extra_indexes.is_empty() {
        ddl += "\n";
        for index in extra_indexes {
            ddl += &format!("\n{};", index.definition);
        }
    }
    if !triggers.is_empty() {
        ddl += "\n";
        for trigger in &triggers {
            ddl += &format!("\n{};", trigger.definition);
        }
    }

    Ok(TableStructure {
        meta,
        columns,
        indexes,
        constraints,
        foreign_keys,
        referenced_by,
        triggers,
        privileges: Vec::new(),
        ddl,
        warnings,
    })
}

fn table_indexes(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<IndexInfo>> {
    let mut stmt = c.prepare(&format!("PRAGMA {}.index_list({})", quote(db), quote(table)))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>("name")?,
            r.get::<_, i64>("unique")? != 0,
            r.get::<_, String>("origin")?,
            r.get::<_, i64>("partial")? != 0,
        ))
    })?;
    let mut indexes = Vec::new();
    for row in rows {
        let (name, is_unique, origin, partial) = row?;
        let sql = stored_sql(c, db, "'index'", &name)?.and_then(|(_, sql)| sql);
        // Expressions have no column name; their text comes from the index's SQL
        let items = sql.as_deref().map(index_items).unwrap_or_default();
        let mut keys = c.prepare(&format!("PRAGMA {}.index_info({})", quote(db), quote(&name)))?;
        let columns = keys
            .query_map([], |r| Ok((r.get::<_, i64>("seqno")?, r.get::<_, i64>("cid")?, r.get::<_, Option<String>>("name")?)))?
            .map(|key| {
                let (seqno, cid, column) = key?;
                Ok(match column {
                    Some(column) => column,
                    None if cid == -1 => "rowid".to_string(),
                    None => items.get(seqno as usize).cloned().unwrap_or_else(|| "(expression)".into()),
                })
            })
            .collect::<rusqlite::Result<Vec<String>>>()?;
        let is_primary = origin == "pk";
        let definition = match &sql {
            Some(sql) => sql.clone(),
            None => {
                let quoted = columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
                format!("{} ({quoted})", if is_primary { "PRIMARY KEY" } else { "UNIQUE" })
            }
        };
        indexes.push(IndexInfo {
            name,
            columns,
            method: "btree".into(),
            is_unique,
            is_primary,
            backs_constraint: origin != "c",
            predicate: if partial { sql.as_deref().and_then(where_clause) } else { None },
            definition,
            bytes: None,
        });
    }
    indexes.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then_with(|| a.name.cmp(&b.name)));
    Ok(indexes)
}

/// Every table of `db` whose foreign keys point at `table`.
fn referencing_tables(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<ReferencedBy>> {
    let mut found = Vec::new();
    for other in objects(c, db)?.into_iter().filter(|o| o.kind == "table") {
        for key in foreign_keys(c, db, &other.name)? {
            if key.table.eq_ignore_ascii_case(table) {
                found.push(ReferencedBy { name: key.name(), schema: db.to_string(), table: other.name.clone(), columns: key.from });
            }
        }
    }
    Ok(found)
}

fn table_triggers(c: &Connection, db: &str, table: &str) -> rusqlite::Result<Vec<TriggerInfo>> {
    let sql = format!(
        "SELECT name, sql FROM {}.sqlite_master WHERE type = 'trigger' AND tbl_name = ?1 COLLATE NOCASE ORDER BY name",
        quote(db)
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))?;
    rows.map(|row| row.map(|(name, sql)| trigger_info(name, sql))).collect()
}

fn completion_schema(c: &Connection) -> rusqlite::Result<CompletionSchema> {
    let mut relations = Vec::new();
    for db in databases(c)? {
        let mut found: Vec<Object> = objects(c, &db)?.into_iter().filter(|o| o.kind == "table" || o.kind == "view").collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        for o in found {
            // A view whose tables are gone has no columns to offer, but the others still do
            let columns = raw_columns(c, &db, &o.name)
                .unwrap_or_default()
                .into_iter()
                .filter(|r| r.hidden != 1)
                .map(|r| CompletionColumn { name: r.name, data_type: r.declared })
                .collect();
            relations.push(CompletionRelation { schema: db.clone(), name: o.name, relation_type: o.kind, columns });
        }
    }
    Ok(CompletionSchema { current_schema: Some("main".into()), relations, functions: Vec::new() })
}

// Reading the SQL SQLite keeps: just enough to tell a trigger's timing and event, a
// partial index's WHERE clause and the text of an index's expressions.

#[derive(Clone, Copy, PartialEq, Debug)]
enum Token<'a> {
    Word(&'a str),
    /// A string or a quoted name.
    Quoted,
    Symbol(u8),
}

/// The tokens of `sql` with their byte ranges, leaving out spaces and comments.
fn tokens(sql: &str) -> Vec<(Token<'_>, usize, usize)> {
    let b = sql.as_bytes();
    let word_byte = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80;
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        match b[i] {
            c if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                continue;
            }
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        if b.get(i + 1) == Some(&q) {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push((Token::Quoted, start, i));
            }
            b'[' => {
                while i < b.len() && b[i] != b']' {
                    i += 1;
                }
                i = (i + 1).min(b.len());
                out.push((Token::Quoted, start, i));
            }
            c if word_byte(c) => {
                while i < b.len() && word_byte(b[i]) {
                    i += 1;
                }
                out.push((Token::Word(&sql[start..i]), start, i));
            }
            c => {
                i += 1;
                out.push((Token::Symbol(c), start, i));
            }
        }
    }
    out
}

fn is_word(token: Option<&(Token<'_>, usize, usize)>, word: &str) -> bool {
    matches!(token, Some((Token::Word(w), ..)) if w.eq_ignore_ascii_case(word))
}

/// What follows the WHERE of `CREATE INDEX ... WHERE ...`.
fn where_clause(sql: &str) -> Option<String> {
    let mut depth = 0;
    for (token, _, end) in tokens(sql) {
        match token {
            Token::Symbol(b'(') => depth += 1,
            Token::Symbol(b')') => depth -= 1,
            Token::Word(w) if depth == 0 && w.eq_ignore_ascii_case("WHERE") => return Some(sql[end..].trim().to_string()),
            _ => {}
        }
    }
    None
}

/// The indexed columns and expressions of `CREATE INDEX ... ON t (a, lower(b) DESC)`, as written.
fn index_items(sql: &str) -> Vec<String> {
    let all = tokens(sql);
    let Some(on) = all.iter().position(|t| is_word(Some(t), "ON")) else {
        return Vec::new();
    };
    let Some(open) = all[on..].iter().position(|(t, ..)| *t == Token::Symbol(b'(')).map(|p| p + on) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let (mut depth, mut item_start) = (0, all[open].2);
    for &(token, start, end) in &all[open + 1..] {
        match token {
            Token::Symbol(b'(') => depth += 1,
            Token::Symbol(b')') if depth > 0 => depth -= 1,
            Token::Symbol(b',' | b')') if depth == 0 => {
                items.push(sql[item_start..start].trim().to_string());
                if token == Token::Symbol(b')') {
                    break;
                }
                item_start = end;
            }
            _ => {}
        }
    }
    items
}

/// A trigger as the Structure tab shows it, read from its `CREATE TRIGGER`.
fn trigger_info(name: String, sql: String) -> TriggerInfo {
    let all = tokens(&sql);
    let word = |i: usize, w: &str| is_word(all.get(i), w);
    let mut i = all.iter().position(|t| is_word(Some(t), "TRIGGER")).map_or(all.len(), |p| p + 1);
    if word(i, "IF") && word(i + 1, "NOT") && word(i + 2, "EXISTS") {
        i += 3;
    }
    // The trigger's name, maybe after its database's
    i += 1;
    if matches!(all.get(i), Some((Token::Symbol(b'.'), ..))) {
        i += 2;
    }
    let timing = if word(i, "BEFORE") {
        i += 1;
        "BEFORE"
    } else if word(i, "AFTER") {
        i += 1;
        "AFTER"
    } else if word(i, "INSTEAD") && word(i + 1, "OF") {
        i += 2;
        "INSTEAD OF"
    } else {
        // What SQLite does when the trigger doesn't say
        "BEFORE"
    };
    let events = ["INSERT", "UPDATE", "DELETE"].into_iter().filter(|e| word(i, e)).map(String::from).collect();
    TriggerInfo {
        name,
        timing: timing.into(),
        events,
        // SQLite only has row triggers
        level: "ROW".into(),
        function: String::new(),
        enabled: true,
        definition: sql,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::tests_support::TempDb;
    use super::super::OpenOptions;
    use super::*;
    use crate::db::model::ExplorerSchema;

    const FIXTURE: &str = "
        CREATE TABLE authors (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, name VARCHAR(80));
        CREATE TABLE regions (country TEXT, code TEXT, PRIMARY KEY (country, code)) WITHOUT ROWID;
        CREATE TABLE posts (
            id INTEGER PRIMARY KEY,
            author_id INTEGER REFERENCES authors (id) ON DELETE CASCADE,
            country TEXT,
            region TEXT,
            title TEXT NOT NULL DEFAULT 'untitled',
            title_length INTEGER GENERATED ALWAYS AS (length(title)) VIRTUAL,
            title_upper TEXT GENERATED ALWAYS AS (upper(title)) STORED,
            FOREIGN KEY (country, region) REFERENCES regions (country, code)
        );
        CREATE TABLE comments (post_id INTEGER REFERENCES posts, body TEXT);
        CREATE TABLE notes (body TEXT);
        CREATE INDEX posts_title_idx ON posts (lower(title), author_id DESC) WHERE title <> '';
        CREATE VIEW recent_posts AS SELECT id, title FROM posts;
        CREATE TRIGGER posts_touch AFTER UPDATE OF title ON posts
        BEGIN
            UPDATE posts SET country = CASE WHEN new.country IS NULL THEN 'x' ELSE new.country END WHERE id = new.id;
        END;
        INSERT INTO authors (email) VALUES ('a@x.io');
    ";

    async fn open(f: &TempDb) -> Arc<SqliteDriver> {
        SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap()
    }

    fn column<'a>(columns: &'a [ColumnInfo], name: &str) -> &'a ColumnInfo {
        columns.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"))
    }

    fn names(columns: &[ColumnInfo]) -> Vec<&str> {
        columns.iter().map(|c| c.name.as_str()).collect()
    }

    fn schema<'a>(tree: &'a ExplorerTree, name: &str) -> &'a ExplorerSchema {
        tree.schemas.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no schema {name}"))
    }

    /// (name, kind, detail) of every object of a schema.
    fn objects(schema: &ExplorerSchema) -> Vec<(&str, &str, &str)> {
        schema.objects.iter().map(|o| (o.name.as_str(), o.kind.as_str(), o.detail.as_str())).collect()
    }

    /// The `sql` SQLite keeps for an object.
    fn stored_sql(f: &TempDb, name: &str) -> String {
        let conn = rusqlite::Connection::open(f.path()).unwrap();
        conn.query_row("SELECT sql FROM sqlite_master WHERE name = ?1", [name], |r| r.get(0)).unwrap()
    }

    #[tokio::test]
    async fn declared_types_map_to_standard_names() {
        let f = TempDb::with(
            "CREATE TABLE typed (a JSON, b DATE, c DATETIME, d TIMESTAMP, e TIME, f INTEGER, g VARCHAR(80), \
             h CLOB, i TEXT, j BLOB, k, l REAL, m FLOAT, n DOUBLE, o BOOLEAN, p NUMERIC(10,2))",
        );
        let d = open(&f).await;
        let columns = d.columns("main", "typed").await.unwrap();
        let types: Vec<(&str, &str, &str)> =
            columns.iter().map(|c| (c.name.as_str(), c.type_name.as_str(), c.data_type.as_str())).collect();
        assert_eq!(
            types,
            vec![
                ("rowid", "INTEGER", "bigint"),
                ("a", "JSON", "json"),
                ("b", "DATE", "date"),
                ("c", "DATETIME", "timestamp without time zone"),
                ("d", "TIMESTAMP", "timestamp without time zone"),
                ("e", "TIME", "time without time zone"),
                ("f", "INTEGER", "bigint"),
                ("g", "VARCHAR(80)", "text"),
                ("h", "CLOB", "text"),
                ("i", "TEXT", "text"),
                ("j", "BLOB", "bytea"),
                ("k", "", "bytea"),
                ("l", "REAL", "double precision"),
                ("m", "FLOAT", "double precision"),
                ("n", "DOUBLE", "double precision"),
                ("o", "BOOLEAN", "numeric"),
                ("p", "NUMERIC(10,2)", "numeric"),
            ]
        );
        assert!(columns.iter().all(|c| c.enum_values.is_none() && c.comment.is_none()));
    }

    #[tokio::test]
    async fn explorer_lists_tables_views_indexes_and_triggers_without_internal_ones() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        let tree = d.explorer().await.unwrap();
        assert_eq!(tree.default_schema.as_deref(), Some("main"));
        let schemas: Vec<&str> = tree.schemas.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(schemas, vec!["main"], "temp without objects isn't listed");
        let main = objects(schema(&tree, "main"));
        assert_eq!(
            main,
            vec![
                ("posts_title_idx", "index", "posts"),
                ("authors", "table", ""),
                ("comments", "table", ""),
                ("notes", "table", ""),
                ("posts", "table", ""),
                ("regions", "table", "without rowid"),
                ("posts_touch", "trigger", "posts"),
                ("recent_posts", "view", ""),
            ]
        );
        assert!(schema(&tree, "main").objects.iter().all(|o| o.partitions.is_empty()));

        d.tables().run(|c| c.execute_batch("CREATE TEMP TABLE scratch (x)").map_err(|e| e.to_string())).await.unwrap();
        let tree = d.explorer().await.unwrap();
        assert_eq!(objects(schema(&tree, "temp")), vec![("scratch", "table", "")], "temp with objects is listed");
    }

    #[tokio::test]
    async fn attached_databases_become_groups() {
        let f = TempDb::with("CREATE TABLE local (id INTEGER PRIMARY KEY)");
        let other = TempDb::with(
            "CREATE TABLE \"odd \"\"name\"\" table\" (\"a \"\"b\"\"\" TEXT PRIMARY KEY, \"c d\" INTEGER); \
             CREATE INDEX \"odd idx\" ON \"odd \"\"name\"\" table\" (\"c d\");",
        );
        let d = open(&f).await;
        let path = other.path().display().to_string();
        d.tables()
            .run(move |c| c.execute("ATTACH ?1 AS \"archive \"\"2025\"\"\"", [path]).map(|_| ()).map_err(|e| e.to_string()))
            .await
            .unwrap();
        let tree = d.explorer().await.unwrap();
        let schemas: Vec<&str> = tree.schemas.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(schemas, vec!["main", "archive \"2025\""]);
        assert_eq!(objects(schema(&tree, "main")), vec![("local", "table", "")]);
        assert_eq!(
            objects(schema(&tree, "archive \"2025\"")),
            vec![("odd idx", "index", "odd \"name\" table"), ("odd \"name\" table", "table", "")]
        );

        // Names that need quoting work in every call
        let db = "archive \"2025\"";
        let table = "odd \"name\" table";
        let columns = d.columns(db, table).await.unwrap();
        assert_eq!(names(&columns), vec!["a \"b\"", "c d"]);
        assert!(column(&columns, "a \"b\"").is_primary_key);
        assert_eq!(d.relation_kind(db, table).await.unwrap().as_deref(), Some("table"));
        let s = d.table_structure(db, table).await.unwrap();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.indexes.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["sqlite_autoindex_odd \"name\" table_1", "odd idx"]);
        assert_eq!(s.indexes[1].columns, vec!["c d"]);
        let def = d.object_definition(db, "odd idx", "index").await.unwrap();
        assert!(def.ddl.starts_with("CREATE INDEX \"odd idx\""), "{}", def.ddl);

        let completion = d.completion_schema().await.unwrap();
        let attached = completion.relations.iter().find(|r| r.schema == db && r.name == table).unwrap();
        assert_eq!(attached.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["a \"b\"", "c d"]);
    }

    #[tokio::test]
    async fn columns_know_keys_generated_columns_and_foreign_keys() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        let columns = d.columns("main", "posts").await.unwrap();
        assert_eq!(names(&columns), vec!["id", "author_id", "country", "region", "title", "title_length", "title_upper"]);

        let id = column(&columns, "id");
        assert!(id.is_primary_key && !id.is_generated);
        assert_eq!((id.type_name.as_str(), id.data_type.as_str()), ("INTEGER", "bigint"));
        assert!(!column(&columns, "author_id").is_primary_key);

        assert!(column(&columns, "title_length").is_generated, "virtual generated");
        assert!(column(&columns, "title_upper").is_generated, "stored generated");
        let title = column(&columns, "title");
        assert!(!title.is_generated && !title.is_nullable);
        assert_eq!(title.default_value.as_deref(), Some("'untitled'"));
        assert!(column(&columns, "country").is_nullable);

        let author = column(&columns, "author_id").foreign_key.clone().unwrap();
        assert_eq!((author.schema.as_str(), author.table.as_str()), ("main", "authors"));
        assert_eq!((author.columns, author.ref_columns), (vec!["author_id".to_string()], vec!["id".to_string()]));

        // Both columns of a composite key point at the same key, in the declared order
        let region = column(&columns, "region").foreign_key.clone().unwrap();
        assert_eq!(region, column(&columns, "country").foreign_key.clone().unwrap());
        assert_eq!(region.table, "regions");
        assert_eq!(region.columns, vec!["country", "region"]);
        assert_eq!(region.ref_columns, vec!["country", "code"]);
        assert!(column(&columns, "title").foreign_key.is_none());

        // A key without columns points at the referenced table's primary key
        let comments = d.columns("main", "comments").await.unwrap();
        let post = column(&comments, "post_id").foreign_key.clone().unwrap();
        assert_eq!((post.table.as_str(), post.ref_columns.clone()), ("posts", vec!["id".to_string()]));

        let view = d.columns("main", "recent_posts").await.unwrap();
        assert_eq!(names(&view), vec!["id", "title"], "views have no rowid column");
        assert!(view.iter().all(|c| !c.is_primary_key));
        assert!(d.columns("main", "missing").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tables_without_a_key_get_a_rowid_column() {
        let f = TempDb::with(
            "CREATE TABLE notes (body TEXT); \
             CREATE TABLE has_rowid (rowid TEXT, x); \
             CREATE TABLE has_two (ROWID TEXT, _rowid_ TEXT, x); \
             CREATE TABLE has_all (rowid TEXT, _rowid_ TEXT, oid TEXT);",
        );
        let d = open(&f).await;
        let notes = d.columns("main", "notes").await.unwrap();
        assert_eq!(names(&notes), vec!["rowid", "body"]);
        let rowid = &notes[0];
        assert!(rowid.is_primary_key && rowid.is_generated && !rowid.is_nullable);
        assert_eq!((rowid.type_name.as_str(), rowid.data_type.as_str()), ("INTEGER", "bigint"));
        assert!(rowid.foreign_key.is_none() && rowid.default_value.is_none());
        assert!(!notes[1].is_primary_key);

        assert_eq!(names(&d.columns("main", "has_rowid").await.unwrap()), vec!["_rowid_", "rowid", "x"]);
        assert_eq!(names(&d.columns("main", "has_two").await.unwrap()), vec!["oid", "ROWID", "_rowid_", "x"]);
        let all = d.columns("main", "has_all").await.unwrap();
        assert_eq!(names(&all), vec!["rowid", "_rowid_", "oid"], "no key left to use");
        assert!(all.iter().all(|c| !c.is_primary_key && !c.is_generated));
    }

    #[tokio::test]
    async fn without_rowid_tables_use_their_key() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        let regions = d.columns("main", "regions").await.unwrap();
        assert_eq!(names(&regions), vec!["country", "code"]);
        assert!(regions.iter().all(|c| c.is_primary_key && !c.is_generated && !c.is_nullable));
        let s = d.table_structure("main", "regions").await.unwrap();
        assert_eq!(s.meta.kind, "table");
        assert!(s.indexes.iter().any(|i| i.is_primary && i.columns == vec!["country", "code"]));
    }

    #[tokio::test]
    async fn structure_keeps_the_original_ddl() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        let s = d.table_structure("main", "posts").await.unwrap();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.meta.kind, "table");
        assert!(s.meta.comment.is_none() && s.meta.total_bytes.is_none() && s.meta.row_estimate.is_none());
        assert_eq!(names(&s.columns), names(&d.columns("main", "posts").await.unwrap()));
        assert!(s.privileges.is_empty());

        let table_sql = stored_sql(&f, "posts");
        let index_sql = stored_sql(&f, "posts_title_idx");
        let trigger_sql = stored_sql(&f, "posts_touch");
        assert_eq!(s.ddl, format!("{table_sql};\n\n{index_sql};\n\n{trigger_sql};"));

        let index = s.indexes.iter().find(|i| i.name == "posts_title_idx").unwrap();
        assert_eq!(index.columns, vec!["lower(title)", "author_id"], "plain columns by name, expressions as written");
        assert_eq!(index.predicate.as_deref(), Some("title <> ''"));
        assert!(!index.is_unique && !index.is_primary && !index.backs_constraint);
        assert_eq!(index.definition, index_sql);

        let fks: Vec<(&str, Vec<String>)> = s.foreign_keys.iter().map(|k| (k.ref_table.as_str(), k.columns.clone())).collect();
        assert_eq!(fks, vec![("authors", vec!["author_id".to_string()]), ("regions", vec!["country".to_string(), "region".to_string()])]);
        assert_eq!((s.foreign_keys[0].on_update.as_str(), s.foreign_keys[0].on_delete.as_str()), ("NO ACTION", "CASCADE"));
        assert_eq!(s.foreign_keys[1].ref_columns, vec!["country", "code"]);

        let trigger = &s.triggers[0];
        assert_eq!((trigger.name.as_str(), trigger.timing.as_str(), trigger.level.as_str()), ("posts_touch", "AFTER", "ROW"));
        assert_eq!(trigger.events, vec!["UPDATE"]);
        assert!(trigger.enabled);
        assert_eq!(trigger.definition, trigger_sql);

        let kinds: Vec<&str> = s.constraints.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["primary key", "foreign key", "foreign key"]);

        let authors = d.table_structure("main", "authors").await.unwrap();
        let referenced: Vec<(&str, &str, Vec<String>)> =
            authors.referenced_by.iter().map(|r| (r.schema.as_str(), r.table.as_str(), r.columns.clone())).collect();
        assert_eq!(referenced, vec![("main", "posts", vec!["author_id".to_string()])]);
        let unique = authors.indexes.iter().find(|i| i.is_unique && !i.is_primary).unwrap();
        assert!(unique.backs_constraint && unique.columns == vec!["email"]);
        assert_eq!(authors.ddl, format!("{};", stored_sql(&f, "authors")), "automatic indexes aren't repeated");
        let kinds: Vec<&str> = authors.constraints.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["primary key", "unique"]);

        // Dropping the table and running the DDL gives the same table back
        let ddl = s.ddl.clone();
        d.tables()
            .run(move |c| {
                let rebuild = format!("PRAGMA foreign_keys = OFF; DROP VIEW recent_posts; DROP TABLE posts; {ddl} PRAGMA foreign_keys = ON;");
                c.execute_batch(&rebuild).map_err(|e| e.to_string())
            })
            .await
            .unwrap();
        let again = d.table_structure("main", "posts").await.unwrap();
        assert_eq!(again.ddl, s.ddl);
        assert_eq!(again.indexes.len(), s.indexes.len());

        let notes = d.table_structure("main", "notes").await.unwrap();
        assert_eq!(notes.ddl, format!("{};", stored_sql(&f, "notes")));
        let missing = d.table_structure("main", "nope").await.err().unwrap();
        assert_eq!(missing, "main.nope doesn't exist");
    }

    #[tokio::test]
    async fn views_indexes_and_triggers_have_their_definition_and_kind() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        let view = d.table_structure("main", "recent_posts").await.unwrap();
        assert_eq!(view.meta.kind, "view");
        assert_eq!(view.ddl, format!("{};", stored_sql(&f, "recent_posts")));
        assert_eq!(names(&view.columns), vec!["id", "title"]);

        let kind = |name: &'static str| {
            let d = d.clone();
            async move { d.relation_kind("main", name).await.unwrap() }
        };
        assert_eq!(kind("posts").await.as_deref(), Some("table"));
        assert_eq!(kind("recent_posts").await.as_deref(), Some("view"));
        assert_eq!(kind("posts_title_idx").await.as_deref(), Some("index"));
        assert_eq!(kind("posts_touch").await.as_deref(), Some("trigger"));
        assert_eq!(kind("missing").await, None);
        assert_eq!(d.relation_kind("nowhere", "posts").await.unwrap(), None);

        let index = d.object_definition("main", "posts_title_idx", "index").await.unwrap();
        assert_eq!(index.kind, "index");
        assert_eq!(index.ddl, format!("{};", stored_sql(&f, "posts_title_idx")));
        assert_eq!(index.facts, vec![("Table".to_string(), "posts".to_string())]);
        assert!(index.comment.is_none());
        let trigger = d.object_definition("main", "posts_touch", "trigger").await.unwrap();
        assert_eq!(trigger.ddl, format!("{};", stored_sql(&f, "posts_touch")));
        let table = d.object_definition("main", "notes", "table").await.unwrap();
        assert_eq!(table.ddl, format!("{};", stored_sql(&f, "notes")));
        assert!(d.object_definition("main", "nope", "index").await.is_err());

        assert_eq!(d.function_detail("main", "f", "").await.err().unwrap(), "Functions isn't supported by SQLite");
        assert_eq!(d.extensions().await.err().unwrap(), "Extensions isn't supported by SQLite");
    }

    #[test]
    fn trigger_headers_name_their_timing_and_events() {
        let read = |sql: &str| {
            let t = trigger_info("t".into(), sql.into());
            (t.timing, t.events)
        };
        let at = |timing: &str, event: &str| (timing.to_string(), vec![event.to_string()]);
        assert_eq!(read("CREATE TRIGGER t AFTER UPDATE OF a, b ON x BEGIN SELECT 1; END"), at("AFTER", "UPDATE"));
        assert_eq!(read("CREATE TRIGGER IF NOT EXISTS \"after\" INSTEAD OF DELETE ON v BEGIN SELECT 1; END"), at("INSTEAD OF", "DELETE"));
        assert_eq!(read("CREATE TRIGGER main.[before] INSERT ON x BEGIN SELECT 1; END"), at("BEFORE", "INSERT"));
        assert_eq!(read("CREATE TEMP TRIGGER /* note */ t BEFORE INSERT ON x BEGIN SELECT 1; END"), at("BEFORE", "INSERT"));
    }

    #[test]
    fn index_sql_gives_expressions_and_the_where_clause() {
        let sql = "CREATE INDEX i ON \"t (x)\" (a, substr(b, 1, 2) COLLATE NOCASE, \"c,d\") WHERE (a > 0) AND b = 'where'";
        assert_eq!(index_items(sql), vec!["a", "substr(b, 1, 2) COLLATE NOCASE", "\"c,d\""]);
        assert_eq!(where_clause(sql).as_deref(), Some("(a > 0) AND b = 'where'"));
        assert_eq!(where_clause("CREATE INDEX i ON t (a)"), None);
    }

    #[tokio::test]
    async fn a_broken_view_does_not_break_autocomplete() {
        let f = TempDb::with("CREATE TABLE t (a); CREATE VIEW v AS SELECT a FROM t; DROP TABLE t; CREATE TABLE u (b);");
        let d = open(&f).await;
        let completion = d.completion_schema().await.unwrap();
        let relations: Vec<(&str, usize)> = completion.relations.iter().map(|r| (r.name.as_str(), r.columns.len())).collect();
        assert_eq!(relations, vec![("u", 1), ("v", 0)]);
    }

    #[tokio::test]
    async fn completion_lists_tables_and_views_of_every_database() {
        let f = TempDb::with(FIXTURE);
        let d = open(&f).await;
        d.tables().run(|c| c.execute_batch("CREATE TEMP TABLE scratch (x INTEGER)").map_err(|e| e.to_string())).await.unwrap();
        let completion = d.completion_schema().await.unwrap();
        assert_eq!(completion.current_schema.as_deref(), Some("main"));
        assert!(completion.functions.is_empty());
        let relations: Vec<(&str, &str, &str)> =
            completion.relations.iter().map(|r| (r.schema.as_str(), r.name.as_str(), r.relation_type.as_str())).collect();
        assert_eq!(
            relations,
            vec![
                ("main", "authors", "table"),
                ("main", "comments", "table"),
                ("main", "notes", "table"),
                ("main", "posts", "table"),
                ("main", "recent_posts", "view"),
                ("main", "regions", "table"),
                ("temp", "scratch", "table"),
            ]
        );
        let authors = &completion.relations[0];
        let columns: Vec<(&str, &str)> = authors.columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str())).collect();
        assert_eq!(columns, vec![("id", "INTEGER"), ("email", "TEXT"), ("name", "VARCHAR(80)")]);
    }
}
