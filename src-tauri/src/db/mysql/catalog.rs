//! What a MySQL database holds: the explorer, columns, object definitions and autocomplete.
//! The "schema" of the contract is the name of a database, and every statement names its
//! tables with it (`` `db`.`t` ``); the session's current database never decides anything.
//! The catalog is read from `information_schema`, aliasing every column: MySQL 8 answers
//! them in upper case otherwise.

use std::collections::HashMap;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Row, Value};

use super::structure;
use super::MysqlDriver;
use crate::db::model::{
    ColumnInfo, CompletionColumn, CompletionRelation, CompletionSchema, Diagram, DiagramColumn, DiagramRelation, DiagramTable,
    ExplorerObject, ExplorerSchema, ExplorerTree, ExtensionInfo, ForeignKeyRef, FunctionDetail, FunctionInfo, ObjectDefinition,
    TableStructure,
};
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;
use crate::db::sql::Dialect;
use crate::db::{unsupported, Catalog, Engine};

#[async_trait]
impl Catalog for MysqlDriver {
    async fn explorer(&self) -> Result<ExplorerTree, String> {
        let database = self.database();
        if database.is_empty() {
            return Ok(ExplorerTree { schemas: Vec::new(), default_schema: None });
        }
        let mut conn = self.conn().await?;
        let objects = explorer_objects(&mut conn, database).await.map_err(|e| self.message(&e))?;
        Ok(ExplorerTree {
            schemas: vec![ExplorerSchema { name: database.to_string(), objects }],
            default_schema: Some(database.to_string()),
        })
    }

    async fn diagram(&self, schema: &str) -> Result<Diagram, String> {
        let mut conn = self.conn().await?;
        diagram(&mut conn, schema).await.map_err(|e| self.message(&e))
    }

    async fn columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let mut conn = self.conn().await?;
        table_columns(&mut conn, schema, table).await.map_err(|e| self.message(&e))
    }

    async fn relation_kind(&self, schema: &str, name: &str) -> Result<Option<String>, String> {
        let mut conn = self.conn().await?;
        let row: Option<Row> = conn
            .exec_first(
                "SELECT TABLE_TYPE AS table_type FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')",
                (schema, name),
            )
            .await
            .map_err(|e| self.message(&e))?;
        Ok(row.map(|r| if string(&r, "table_type") == "VIEW" { "view" } else { "table" }.to_string()))
    }

    async fn table_structure(&self, schema: &str, table: &str) -> Result<TableStructure, String> {
        let mut conn = self.conn().await?;
        structure::table_structure(&mut conn, self.endpoint(), self.version(), schema, table).await
    }

    async fn completion_schema(&self) -> Result<CompletionSchema, String> {
        let database = self.database();
        if database.is_empty() {
            return Ok(CompletionSchema { current_schema: None, relations: Vec::new(), functions: Vec::new() });
        }
        let mut conn = self.conn().await?;
        completion_schema(&mut conn, database).await.map_err(|e| self.message(&e))
    }

    async fn object_definition(&self, schema: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String> {
        let mut conn = self.conn().await?;
        object_definition(&mut conn, self.endpoint(), schema, name, kind).await
    }

    async fn function_detail(&self, schema: &str, name: &str, args: &str) -> Result<FunctionDetail, String> {
        let mut conn = self.conn().await?;
        function_detail(&mut conn, self.endpoint(), schema, name, args).await
    }

    async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String> {
        Err(unsupported(Engine::Mysql, "Extensions"))
    }
}

impl MysqlDriver {
    /// An error of the server in the app's words.
    fn message(&self, e: &mysql_async::Error) -> String {
        error_message(e, self.endpoint())
    }
}

// Reading rows by the name they were aliased to

/// The cell called `name`.
fn cell<'a>(row: &'a Row, name: &str) -> Option<&'a Value> {
    let at = row.columns_ref().iter().position(|c| &*c.name_str() == name)?;
    row.as_ref(at)
}

pub(super) fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::NULL => None,
        Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::Int(n) => Some(n.to_string()),
        Value::UInt(n) => Some(n.to_string()),
        Value::Float(n) => Some(n.to_string()),
        Value::Double(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The text of the cell `name`; None for NULL.
pub(super) fn text(row: &Row, name: &str) -> Option<String> {
    cell(row, name).and_then(value_text)
}

/// The text of the cell `name`; empty for NULL.
pub(super) fn string(row: &Row, name: &str) -> String {
    text(row, name).unwrap_or_default()
}

/// The number in the cell `name`; None for NULL.
pub(super) fn number(row: &Row, name: &str) -> Option<i64> {
    match cell(row, name)? {
        Value::Int(n) => Some(*n),
        Value::UInt(n) => i64::try_from(*n).ok(),
        Value::Float(n) => Some(*n as i64),
        Value::Double(n) => Some(*n as i64),
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).trim().parse().ok(),
        _ => None,
    }
}

/// `` `name` ``.
pub(super) fn quote(name: &str) -> String {
    Dialect::Mysql.quote_ident(name)
}

/// `` `schema`.`name` ``.
pub(super) fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", quote(schema), quote(name))
}

/// A text that is empty when the server has nothing to say.
pub(super) fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

// Columns

/// A column as MySQL stores it, for the generators of `MODIFY` and the binder.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnDefinition {
    pub name: String,
    /// `varchar(80)`, `int unsigned`, `enum('a','b')`.
    pub column_type: String,
    /// `varchar`, `int`, `enum`, `point`.
    pub data_type: String,
    pub charset: Option<String>,
    pub collation: Option<String>,
    pub nullable: bool,
    /// As MySQL keeps it, without quotes; an expression when `extra` says `DEFAULT_GENERATED`.
    pub default: Option<String>,
    /// `auto_increment`, `on update CURRENT_TIMESTAMP`, `STORED GENERATED`, `INVISIBLE`...
    pub extra: String,
    pub generation_expression: Option<String>,
    pub comment: String,
}

impl ColumnDefinition {
    /// Computed by the server, so it can't be written. `auto_increment` can.
    fn is_generated(&self) -> bool {
        let extra = self.extra.to_ascii_lowercase();
        extra.contains("virtual generated") || extra.contains("stored generated")
    }
}

/// A column with the key it belongs to (`PRI`, `UNI`, `MUL` or empty).
pub(super) struct StoredColumn {
    pub definition: ColumnDefinition,
    pub key: String,
}

pub(super) async fn read_columns(conn: &mut Conn, database: &str, table: &str) -> mysql_async::Result<Vec<StoredColumn>> {
    let rows: Vec<Row> = conn
        .exec(
            "SELECT COLUMN_NAME AS column_name, COLUMN_DEFAULT AS column_default, IS_NULLABLE AS is_nullable, \
                    DATA_TYPE AS data_type, COLUMN_TYPE AS column_type, COLUMN_KEY AS column_key, EXTRA AS extra, \
                    COLUMN_COMMENT AS column_comment, CHARACTER_SET_NAME AS charset_name, COLLATION_NAME AS collation_name, \
                    GENERATION_EXPRESSION AS generation_expression \
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
            (database, table),
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| StoredColumn {
            definition: ColumnDefinition {
                name: string(row, "column_name"),
                column_type: string(row, "column_type"),
                data_type: string(row, "data_type"),
                charset: text(row, "charset_name"),
                collation: text(row, "collation_name"),
                nullable: string(row, "is_nullable") == "YES",
                default: text(row, "column_default"),
                extra: string(row, "extra"),
                generation_expression: text(row, "generation_expression").and_then(non_empty),
                comment: string(row, "column_comment"),
            },
            key: string(row, "column_key"),
        })
        .collect())
}

/// The columns of `database`.`table` in order, as MySQL stores them.
pub(crate) async fn column_definitions(conn: &mut Conn, database: &str, table: &str) -> Result<Vec<ColumnDefinition>, String> {
    let stored = read_columns(conn, database, table).await.map_err(|e| detached(&e))?;
    Ok(stored.into_iter().map(|c| c.definition).collect())
}

/// The storage engine of a table (`InnoDB`, `MyISAM`...); None for a view or a table that isn't there.
pub(crate) async fn table_engine(conn: &mut Conn, database: &str, table: &str) -> Result<Option<String>, String> {
    let row: Option<Row> = conn
        .exec_first(
            "SELECT ENGINE AS engine FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
            (database, table),
        )
        .await
        .map_err(|e| detached(&e))?;
    Ok(row.and_then(|r| text(&r, "engine")))
}

/// What kind each constraint of a table is, by name: `PRIMARY KEY` (named `PRIMARY`),
/// `UNIQUE`, `FOREIGN KEY` or `CHECK`.
#[allow(dead_code)] // used by dropping constraints (Task 11)
pub(crate) async fn constraint_kinds(conn: &mut Conn, database: &str, table: &str) -> Result<HashMap<String, String>, String> {
    let rows = structure::read_constraint_types(conn, database, table).await.map_err(|e| detached(&e))?;
    Ok(rows.into_iter().collect())
}

/// An error where the session's endpoint isn't at hand: the server's own errors read the same.
fn detached(e: &mysql_async::Error) -> String {
    error_message(e, &MysqlEndpoint::default())
}

/// The values of an `enum('a','b')` or `set('a','b')` column type, with doubled quotes and
/// backslash escapes resolved; None for any other type.
pub(super) fn enum_values(column_type: &str) -> Option<Vec<String>> {
    let lower = column_type.to_ascii_lowercase();
    let start = if lower.starts_with("enum(") {
        5
    } else if lower.starts_with("set(") {
        4
    } else {
        return None;
    };
    let mut values = Vec::new();
    let mut chars = column_type[start..].chars().peekable();
    while let Some(c) = chars.next() {
        // Commas, spaces and the closing parenthesis sit between the values
        if c != '\'' {
            continue;
        }
        let mut value = String::new();
        loop {
            match chars.next() {
                Some('\'') if chars.peek() == Some(&'\'') => {
                    chars.next();
                    value.push('\'');
                }
                Some('\'') | None => break,
                Some('\\') => match chars.next() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('0') => value.push('\0'),
                    Some('b') => value.push('\u{8}'),
                    Some('Z') => value.push('\u{1a}'),
                    Some(other) => value.push(other),
                    None => break,
                },
                Some(other) => value.push(other),
            }
        }
        values.push(value);
    }
    Some(values)
}

pub(super) async fn table_columns(conn: &mut Conn, database: &str, table: &str) -> mysql_async::Result<Vec<ColumnInfo>> {
    let stored = read_columns(conn, database, table).await?;
    let keys = structure::read_foreign_keys(conn, "k.TABLE_SCHEMA = ? AND k.TABLE_NAME = ?", (database, table)).await?;
    Ok(stored
        .into_iter()
        .enumerate()
        .map(|(i, StoredColumn { definition, key })| {
            let foreign_key = keys.iter().find(|k| k.columns.contains(&definition.name)).map(|k| ForeignKeyRef {
                constraint: k.name.clone(),
                schema: k.ref_schema.clone(),
                table: k.ref_table.clone(),
                columns: k.columns.clone(),
                ref_columns: k.ref_columns.clone(),
            });
            ColumnInfo {
                is_generated: definition.is_generated(),
                enum_values: enum_values(&definition.column_type),
                is_primary_key: key == "PRI",
                ordinal_position: i as i32 + 1,
                comment: non_empty(definition.comment),
                default_value: definition.default,
                is_nullable: definition.nullable,
                type_name: definition.column_type,
                data_type: definition.data_type,
                name: definition.name,
                foreign_key,
            }
        })
        .collect())
}

// The explorer

/// The parameters and the return type of a stored routine, as the explorer and the
/// routine's page print them.
#[derive(Default, Clone)]
pub(super) struct Signature {
    /// `IN id int, OUT total decimal(10,2)` for a procedure, `o int` for a function.
    pub arguments: String,
    /// `decimal(10,2)`; empty for a procedure.
    pub returns: String,
}

/// The signature of every routine of `database` by (name, `PROCEDURE` or `FUNCTION`).
pub(super) async fn read_signatures(conn: &mut Conn, database: &str) -> mysql_async::Result<HashMap<(String, String), Signature>> {
    let rows: Vec<Row> = conn
        .exec(
            "SELECT SPECIFIC_NAME AS routine_name, ROUTINE_TYPE AS routine_type, ORDINAL_POSITION AS position, \
                    PARAMETER_MODE AS mode, PARAMETER_NAME AS parameter_name, DTD_IDENTIFIER AS data_type \
             FROM information_schema.PARAMETERS WHERE SPECIFIC_SCHEMA = ? \
             ORDER BY SPECIFIC_NAME, ROUTINE_TYPE, ORDINAL_POSITION",
            (database,),
        )
        .await?;
    let mut found: HashMap<(String, String), Signature> = HashMap::new();
    for row in &rows {
        let signature = found.entry((string(row, "routine_name"), string(row, "routine_type"))).or_default();
        let data_type = string(row, "data_type");
        // Position 0 is what a function returns
        if number(row, "position") == Some(0) {
            signature.returns = data_type;
            continue;
        }
        let mut argument = String::new();
        // Only a procedure's parameters have a direction; a function's all go in
        let mode = text(row, "mode").filter(|_| string(row, "routine_type") == "PROCEDURE");
        for part in [mode, text(row, "parameter_name")].into_iter().flatten() {
            argument += &part;
            argument.push(' ');
        }
        argument += &data_type;
        if !signature.arguments.is_empty() {
            signature.arguments += ", ";
        }
        signature.arguments += &argument;
    }
    Ok(found)
}

async fn explorer_objects(conn: &mut Conn, database: &str) -> mysql_async::Result<Vec<ExplorerObject>> {
    let relations: Vec<Row> = conn
        .exec(
            "SELECT TABLE_NAME AS name, TABLE_TYPE AS table_type FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') ORDER BY TABLE_NAME",
            (database,),
        )
        .await?;
    let routines: Vec<Row> = conn
        .exec(
            "SELECT ROUTINE_NAME AS name, ROUTINE_TYPE AS routine_type FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME, ROUTINE_TYPE",
            (database,),
        )
        .await?;
    let signatures = read_signatures(conn, database).await?;
    let mut objects: Vec<ExplorerObject> = relations
        .iter()
        .map(|row| ExplorerObject {
            name: string(row, "name"),
            kind: if string(row, "table_type") == "VIEW" { "view" } else { "table" }.into(),
            detail: String::new(),
            partitions: Vec::new(),
        })
        .collect();
    objects.extend(routines.iter().map(|row| {
        let routine_type = string(row, "routine_type");
        let signature = signatures.get(&(string(row, "name"), routine_type.clone())).cloned().unwrap_or_default();
        ExplorerObject {
            name: string(row, "name"),
            kind: routine_type.to_ascii_lowercase(),
            detail: signature.arguments,
            partitions: Vec::new(),
        }
    }));
    objects.sort_by(|a, b| a.name.cmp(&b.name).then(a.kind.cmp(&b.kind)));
    Ok(objects)
}

// Autocomplete

async fn completion_schema(conn: &mut Conn, database: &str) -> mysql_async::Result<CompletionSchema> {
    // A view whose tables are gone has no columns but still is a name to offer
    let rows: Vec<Row> = conn
        .exec(
            "SELECT t.TABLE_NAME AS table_name, t.TABLE_TYPE AS table_type, c.COLUMN_NAME AS column_name, c.COLUMN_TYPE AS column_type \
             FROM information_schema.TABLES t \
             LEFT JOIN information_schema.COLUMNS c ON c.TABLE_SCHEMA = t.TABLE_SCHEMA AND c.TABLE_NAME = t.TABLE_NAME \
             WHERE t.TABLE_SCHEMA = ? AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') \
             ORDER BY t.TABLE_NAME, c.ORDINAL_POSITION",
            (database,),
        )
        .await?;
    let mut relations: Vec<CompletionRelation> = Vec::new();
    for row in &rows {
        let name = string(row, "table_name");
        if relations.last().is_none_or(|last| last.name != name) {
            relations.push(CompletionRelation {
                schema: database.to_string(),
                name,
                relation_type: if string(row, "table_type") == "VIEW" { "view" } else { "table" }.into(),
                columns: Vec::new(),
            });
        }
        if let Some(column) = text(row, "column_name") {
            relations.last_mut().unwrap().columns.push(CompletionColumn { name: column, data_type: string(row, "column_type") });
        }
    }
    let routines: Vec<Row> = conn
        .exec(
            "SELECT ROUTINE_NAME AS name, ROUTINE_TYPE AS routine_type FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME, ROUTINE_TYPE",
            (database,),
        )
        .await?;
    let signatures = read_signatures(conn, database).await?;
    let functions = routines
        .iter()
        .map(|row| {
            let signature = signatures.get(&(string(row, "name"), string(row, "routine_type"))).cloned().unwrap_or_default();
            FunctionInfo {
                name: string(row, "name"),
                schema: database.to_string(),
                return_type: signature.returns,
                argument_types: signature.arguments,
            }
        })
        .collect();
    Ok(CompletionSchema { current_schema: Some(database.to_string()), relations, functions })
}

// The diagram

async fn diagram(conn: &mut Conn, schema: &str) -> mysql_async::Result<Diagram> {
    let keys = structure::read_foreign_keys(
        conn,
        "(k.TABLE_SCHEMA = ? OR k.REFERENCED_TABLE_SCHEMA = ?)",
        (schema, schema),
    )
    .await?;
    let relations: Vec<DiagramRelation> = keys
        .iter()
        .map(|k| DiagramRelation {
            name: k.name.clone(),
            from_schema: k.schema.clone(),
            from_table: k.table.clone(),
            from_columns: k.columns.clone(),
            to_schema: k.ref_schema.clone(),
            to_table: k.ref_table.clone(),
            to_columns: k.ref_columns.clone(),
        })
        .collect();

    // The tables of the schema, and the ones on the other end of a key
    let home: Vec<Row> = conn
        .exec(
            "SELECT TABLE_NAME AS name FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'",
            (schema,),
        )
        .await?;
    let mut wanted: Vec<(String, String)> = home.iter().map(|row| (schema.to_string(), string(row, "name"))).collect();
    for relation in &relations {
        for end in [(&relation.from_schema, &relation.from_table), (&relation.to_schema, &relation.to_table)] {
            let end = (end.0.clone(), end.1.clone());
            if !wanted.contains(&end) {
                wanted.push(end);
            }
        }
    }
    let mut schemas: Vec<&String> = wanted.iter().map(|(s, _)| s).collect();
    schemas.sort();
    schemas.dedup();

    let mut tables: Vec<DiagramTable> = Vec::new();
    for owner in schemas {
        let rows: Vec<Row> = conn
            .exec(
                "SELECT TABLE_NAME AS table_name, COLUMN_NAME AS column_name, COLUMN_TYPE AS column_type, \
                        ORDINAL_POSITION AS position, COLUMN_KEY AS column_key \
                 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
                (owner.as_str(),),
            )
            .await?;
        for row in &rows {
            let name = string(row, "table_name");
            if !wanted.contains(&(owner.clone(), name.clone())) {
                continue;
            }
            let column_name = string(row, "column_name");
            let is_foreign_key = relations.iter().any(|r| {
                (r.from_schema == *owner && r.from_table == name && r.from_columns.contains(&column_name))
                    || (r.to_schema == *owner && r.to_table == name && r.to_columns.contains(&column_name))
            });
            let column = DiagramColumn {
                name: column_name,
                type_name: string(row, "column_type"),
                is_primary_key: string(row, "column_key") == "PRI",
                is_foreign_key,
                ordinal: number(row, "position").unwrap_or_default() as i32,
            };
            match tables.last_mut() {
                Some(last) if last.schema == *owner && last.name == name => last.columns.push(column),
                _ => tables.push(DiagramTable { schema: owner.clone(), name, columns: vec![column] }),
            }
        }
    }
    tables.sort_by(|a, b| (a.schema != schema).cmp(&(b.schema != schema)).then(a.schema.cmp(&b.schema)).then(a.name.cmp(&b.name)));
    Ok(Diagram { tables, relations })
}

// Definitions

/// The text SHOW CREATE gave: `column` of its only row. None when the server hides it
/// (NULL, for a routine the user may not read) or there's no such object.
pub(super) async fn show_create(conn: &mut Conn, what: &str, schema: &str, name: &str, column: usize) -> mysql_async::Result<Option<String>> {
    let row: Option<Row> = conn.query_first(format!("SHOW CREATE {what} {}", qualified(schema, name))).await?;
    Ok(row.and_then(|r| r.as_ref(column).and_then(value_text)))
}

/// `sql` ended with a semicolon, the way the other engines' definitions are.
pub(super) fn terminated(sql: String) -> String {
    let trimmed = sql.trim_end();
    if trimmed.is_empty() || trimmed.ends_with(';') {
        trimmed.to_string()
    } else {
        format!("{trimmed};")
    }
}

fn missing(schema: &str, name: &str) -> String {
    format!("{schema}.{name} doesn't exist")
}

async fn object_definition(conn: &mut Conn, endpoint: &MysqlEndpoint, schema: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String> {
    let fail = |e: mysql_async::Error| error_message(&e, endpoint);
    let (what, column) = match kind {
        "table" => ("TABLE", 1),
        "view" => ("VIEW", 1),
        "procedure" => ("PROCEDURE", 2),
        "function" => ("FUNCTION", 2),
        "trigger" => ("TRIGGER", 2),
        other => return Err(format!("Unknown object kind \"{other}\"")),
    };
    let (facts, comment) = match kind {
        "table" => {
            let row: Option<Row> = conn
                .exec_first(
                    "SELECT ENGINE AS engine, TABLE_COLLATION AS collation_name, TABLE_COMMENT AS table_comment \
                     FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE = 'BASE TABLE'",
                    (schema, name),
                )
                .await
                .map_err(fail)?;
            let row = row.ok_or_else(|| missing(schema, name))?;
            let mut facts = Vec::new();
            for (label, value) in [("Engine", text(&row, "engine")), ("Collation", text(&row, "collation_name"))] {
                facts.extend(value.map(|v| (label.to_string(), v)));
            }
            (facts, non_empty(string(&row, "table_comment")))
        }
        "view" => {
            let row: Option<Row> = conn
                .exec_first(
                    "SELECT DEFINER AS definer, SECURITY_TYPE AS security_type, CHECK_OPTION AS check_option, IS_UPDATABLE AS is_updatable \
                     FROM information_schema.VIEWS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
                    (schema, name),
                )
                .await
                .map_err(fail)?;
            let row = row.ok_or_else(|| missing(schema, name))?;
            let facts = vec![
                ("Definer".to_string(), string(&row, "definer")),
                ("Security".to_string(), string(&row, "security_type")),
                ("Check option".to_string(), string(&row, "check_option")),
                ("Updatable".to_string(), string(&row, "is_updatable")),
            ];
            (facts, None)
        }
        "trigger" => {
            let row: Option<Row> = conn
                .exec_first(
                    "SELECT EVENT_OBJECT_TABLE AS table_name, ACTION_TIMING AS timing, EVENT_MANIPULATION AS event, DEFINER AS definer \
                     FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = ? AND TRIGGER_NAME = ?",
                    (schema, name),
                )
                .await
                .map_err(fail)?;
            let row = row.ok_or_else(|| missing(schema, name))?;
            let facts = vec![
                ("Table".to_string(), string(&row, "table_name")),
                ("Timing".to_string(), string(&row, "timing")),
                ("Event".to_string(), string(&row, "event")),
                ("Definer".to_string(), string(&row, "definer")),
            ];
            (facts, None)
        }
        _ => {
            let routine = read_routine(conn, schema, name, kind.to_ascii_uppercase().as_str()).await.map_err(fail)?;
            let routine = routine.ok_or_else(|| missing(schema, name))?;
            (routine.facts(), non_empty(routine.comment))
        }
    };
    let ddl = show_create(conn, what, schema, name, column).await.map_err(fail)?.unwrap_or_default();
    Ok(ObjectDefinition { kind: kind.into(), facts, comment, ddl: terminated(ddl) })
}

/// A stored routine as `information_schema.routines` describes it.
struct Routine {
    kind: String,
    definer: String,
    security: String,
    deterministic: bool,
    data_access: String,
    language: String,
    comment: String,
    signature: Signature,
}

impl Routine {
    fn facts(&self) -> Vec<(String, String)> {
        let mut facts = vec![("Definer".to_string(), self.definer.clone()), ("Security".to_string(), self.security.clone())];
        if !self.signature.returns.is_empty() {
            facts.push(("Returns".to_string(), self.signature.returns.clone()));
        }
        facts.push(("Deterministic".to_string(), if self.deterministic { "YES" } else { "NO" }.into()));
        facts.push(("Data access".to_string(), self.data_access.clone()));
        facts
    }
}

/// The routine `name` of `schema` of the type `PROCEDURE` or `FUNCTION`.
async fn read_routine(conn: &mut Conn, schema: &str, name: &str, routine_type: &str) -> mysql_async::Result<Option<Routine>> {
    let row: Option<Row> = conn
        .exec_first(
            "SELECT ROUTINE_TYPE AS routine_type, DEFINER AS definer, SECURITY_TYPE AS security_type, \
                    IS_DETERMINISTIC AS is_deterministic, SQL_DATA_ACCESS AS data_access, ROUTINE_BODY AS body, \
                    ROUTINE_COMMENT AS routine_comment \
             FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = ? AND ROUTINE_NAME = ? AND ROUTINE_TYPE = ?",
            (schema, name, routine_type),
        )
        .await?;
    Ok(row.map(|row| Routine {
        kind: string(&row, "routine_type"),
        definer: string(&row, "definer"),
        security: string(&row, "security_type"),
        deterministic: string(&row, "is_deterministic") == "YES",
        data_access: string(&row, "data_access"),
        language: string(&row, "body"),
        comment: string(&row, "routine_comment"),
        signature: Signature::default(),
    }))
}

async fn function_detail(conn: &mut Conn, endpoint: &MysqlEndpoint, schema: &str, name: &str, args: &str) -> Result<FunctionDetail, String> {
    let fail = |e: mysql_async::Error| error_message(&e, endpoint);
    let signatures = read_signatures(conn, schema).await.map_err(fail)?;
    // A procedure and a function may share a name; the arguments tell which one is asked for
    let mut found = Vec::new();
    for routine_type in ["FUNCTION", "PROCEDURE"] {
        if let Some(mut routine) = read_routine(conn, schema, name, routine_type).await.map_err(fail)? {
            routine.signature = signatures.get(&(name.to_string(), routine_type.to_string())).cloned().unwrap_or_default();
            found.push(routine);
        }
    }
    let at = found.iter().position(|r| r.signature.arguments == args).unwrap_or(0);
    if found.is_empty() {
        return Err(missing(schema, name));
    }
    let routine = found.swap_remove(at);
    let what = routine.kind.as_str();
    let source = show_create(conn, what, schema, name, 2).await.map_err(fail)?.unwrap_or_default();
    Ok(FunctionDetail {
        name: name.to_string(),
        schema: schema.to_string(),
        return_type: routine.signature.returns,
        argument_types: routine.signature.arguments,
        language: routine.language,
        source: terminated(source),
        kind: routine.kind.to_ascii_lowercase(),
        volatility: if routine.deterministic { "DETERMINISTIC" } else { "NOT DETERMINISTIC" }.into(),
        is_strict: false,
        owner: routine.definer,
        description: non_empty(routine.comment),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Option<Vec<String>> {
        Some(items.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn enum_and_set_values_resolve_quotes_and_escapes() {
        assert_eq!(enum_values("enum('a','b')"), list(&["a", "b"]));
        assert_eq!(enum_values("enum('a','b''c')"), list(&["a", "b'c"]));
        assert_eq!(enum_values(r"set('x','y z','q\\r')"), list(&["x", "y z", r"q\r"]));
        assert_eq!(enum_values("ENUM('a,b','(c)')"), list(&["a,b", "(c)"]));
        assert_eq!(enum_values(r"enum('line\nbreak','it\'s','')"), list(&["line\nbreak", "it's", ""]));
        assert_eq!(enum_values("enum('''')"), list(&["'"]));
        assert_eq!(enum_values("enum()"), list(&[]));
        assert_eq!(enum_values("varchar(10)"), None);
        assert_eq!(enum_values("int unsigned"), None);
    }

    #[test]
    fn definitions_end_with_one_semicolon() {
        assert_eq!(terminated("CREATE VIEW v AS SELECT 1".into()), "CREATE VIEW v AS SELECT 1;");
        assert_eq!(terminated("CREATE VIEW v AS SELECT 1;  \n".into()), "CREATE VIEW v AS SELECT 1;");
        assert_eq!(terminated(String::new()), "");
    }
}

/// Tests that need a MySQL server, like the ones of the session (see `mod.rs`).
#[cfg(test)]
mod server_tests {
    use std::sync::Arc;

    use super::super::tests_support::{exec, TestDb};
    use super::*;
    use crate::db::model::{ColumnInfo, ExplorerTree};
    use crate::db::Driver;

    const SHOP: &str = include_str!("fixtures/shop.sql");

    /// A database with the shop in it, and a session open on it.
    async fn shop() -> (TestDb, Arc<MysqlDriver>) {
        let db = TestDb::new().await;
        exec(&db.endpoint, SHOP).await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        (db, driver)
    }

    fn column<'a>(columns: &'a [ColumnInfo], name: &str) -> &'a ColumnInfo {
        columns.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"))
    }

    fn objects(tree: &ExplorerTree) -> Vec<(String, String)> {
        tree.schemas[0].objects.iter().map(|o| (o.name.clone(), o.kind.clone())).collect()
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn the_explorer_shows_the_open_database_only() {
        let (db, driver) = shop().await;
        // Another database with a table of its own, which the explorer must leave out
        let other = TestDb::new().await;
        exec(&other.endpoint, "CREATE TABLE elsewhere (x INT)").await;
        let tree = driver.catalog().explorer().await.unwrap();
        assert_eq!(tree.default_schema.as_deref(), Some(db.name.as_str()));
        assert_eq!(tree.schemas.len(), 1);
        assert_eq!(tree.schemas[0].name, db.name);
        let pairs = |names: &[(&str, &str)]| names.iter().map(|(n, k)| (n.to_string(), k.to_string())).collect::<Vec<_>>();
        assert_eq!(
            objects(&tree),
            pairs(&[
                ("big_orders", "view"),
                ("customers", "table"),
                ("order_total", "function"),
                ("orders", "table"),
                ("touch_customer", "procedure"),
            ])
        );
        let detail = |name: &str| tree.schemas[0].objects.iter().find(|o| o.name == name).unwrap().detail.clone();
        assert_eq!(detail("touch_customer"), "IN id int");
        assert_eq!(detail("order_total"), "o int");
        assert_eq!(detail("customers"), "");
        driver.close().await;
        other.drop_now().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn columns_read_types_keys_generated_and_enums() {
        let (db, driver) = shop().await;
        exec(
            &db.endpoint,
            "CREATE TABLE tags (
                n INT NOT NULL AUTO_INCREMENT PRIMARY KEY COMMENT 'it''s the key',
                m INT NULL,
                kind ENUM('a', 'b''c') NULL,
                flags SET('x', 'y z', 'q\\\\r') NOT NULL DEFAULT 'x',
                pt POINT NULL,
                doubled INT GENERATED ALWAYS AS (m * 2) VIRTUAL
            )",
        )
        .await;
        let catalog = driver.catalog();

        let orders = catalog.columns(&db.name, "orders").await.unwrap();
        assert_eq!(orders.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().first(), Some(&"id"));
        assert!(column(&orders, "total_with_tax").is_generated);
        assert!(!column(&orders, "total").is_generated);
        assert!(!column(&orders, "id").is_generated, "auto_increment stays editable");
        let customer_id = column(&orders, "customer_id");
        let key = customer_id.foreign_key.as_ref().unwrap();
        assert_eq!(key.table, "customers");
        assert_eq!(key.schema, db.name);
        assert_eq!(key.constraint, "orders_customer_fk");
        assert_eq!(key.columns, vec!["customer_id".to_string()]);
        assert_eq!(key.ref_columns, vec!["id".to_string()]);
        assert!(column(&orders, "id").foreign_key.is_none());
        assert_eq!(customer_id.type_name, "int");
        assert_eq!(customer_id.data_type, "int");
        assert!(!customer_id.is_nullable);
        assert_eq!(column(&orders, "total").type_name, "decimal(10,2)");
        assert_eq!(column(&orders, "status").default_value.as_deref(), Some("new"));
        assert_eq!(column(&orders, "status").type_name, "enum('new','paid','shipped')");
        assert_eq!(column(&orders, "status").data_type, "enum");
        assert_eq!(
            column(&orders, "status").enum_values,
            Some(vec!["new".to_string(), "paid".to_string(), "shipped".to_string()])
        );
        assert_eq!(column(&orders, "total_with_tax").default_value, None);
        assert_eq!(orders.iter().map(|c| c.ordinal_position).collect::<Vec<_>>(), (1..=orders.len() as i32).collect::<Vec<_>>());

        let customers = catalog.columns(&db.name, "customers").await.unwrap();
        assert!(column(&customers, "id").is_primary_key);
        assert!(!column(&customers, "email").is_primary_key);
        assert_eq!(column(&customers, "email").type_name, "varchar(120)");
        assert_eq!(column(&customers, "email").data_type, "varchar");
        assert!(column(&customers, "age").is_nullable);
        assert_eq!(column(&customers, "name").default_value.as_deref(), Some("anonymous"));
        assert_eq!(column(&customers, "name").comment, None);

        let tags = catalog.columns(&db.name, "tags").await.unwrap();
        assert_eq!(column(&tags, "kind").enum_values, Some(vec!["a".to_string(), "b'c".to_string()]));
        assert_eq!(column(&tags, "kind").type_name, "enum('a','b''c')");
        assert_eq!(column(&tags, "flags").enum_values, Some(vec!["x".to_string(), "y z".to_string(), "q\\r".to_string()]));
        assert_eq!(column(&tags, "n").comment.as_deref(), Some("it's the key"));
        assert!(!column(&tags, "n").is_generated);
        assert!(column(&tags, "doubled").is_generated);
        assert_eq!(column(&tags, "pt").data_type, "point");
        assert_eq!(column(&tags, "n").enum_values, None);

        // A table that isn't there has no columns; a view has its own
        assert!(catalog.columns(&db.name, "nope").await.unwrap().is_empty());
        let view = catalog.columns(&db.name, "big_orders").await.unwrap();
        assert_eq!(view.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "customer_id", "total"]);

        assert_eq!(catalog.relation_kind(&db.name, "orders").await.unwrap().as_deref(), Some("table"));
        assert_eq!(catalog.relation_kind(&db.name, "big_orders").await.unwrap().as_deref(), Some("view"));
        assert_eq!(catalog.relation_kind(&db.name, "nope").await.unwrap(), None);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn the_structure_reads_indexes_constraints_triggers_and_ddl() {
        let (db, driver) = shop().await;
        let catalog = driver.catalog();

        let customers = catalog.table_structure(&db.name, "customers").await.unwrap();
        assert!(customers.warnings.is_empty(), "{:?}", customers.warnings);
        let index = |name: &str| customers.indexes.iter().find(|i| i.name == name).unwrap_or_else(|| panic!("no index {name}"));
        assert_eq!(index("customers_bio_ft").method, "FULLTEXT");
        assert_eq!(index("customers_bio_ft").columns, vec!["bio".to_string()]);
        assert!(!index("customers_bio_ft").is_unique);
        assert!(!index("customers_bio_ft").backs_constraint);
        assert!(index("PRIMARY").is_primary && index("PRIMARY").is_unique && index("PRIMARY").backs_constraint);
        assert_eq!(index("PRIMARY").method, "BTREE");
        assert_eq!(customers.indexes[0].name, "PRIMARY", "the primary key comes first");
        assert!(index("customers_email_uq").is_unique && index("customers_email_uq").backs_constraint);
        assert!(index("customers_bio_ft").definition.starts_with("CREATE FULLTEXT INDEX `customers_bio_ft` ON"), "{}", index("customers_bio_ft").definition);
        assert!(index("customers_email_uq").definition.starts_with("CREATE UNIQUE INDEX"), "{}", index("customers_email_uq").definition);

        let constraint = |name: &str| customers.constraints.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no constraint {name}"));
        assert_eq!(constraint("PRIMARY").kind, "primary key");
        assert_eq!(constraint("PRIMARY").definition, "PRIMARY KEY (`id`)");
        assert_eq!(constraint("customers_email_uq").kind, "unique");
        assert_eq!(constraint("customers_email_uq").definition, "UNIQUE (`email`)");
        assert_eq!(constraint("customers_age_ck").kind, "check");
        assert!(constraint("customers_age_ck").definition.starts_with("CHECK ("), "{}", constraint("customers_age_ck").definition);
        assert!(constraint("customers_age_ck").definition.contains("age"));

        assert!(customers.foreign_keys.is_empty());
        let referenced = customers.referenced_by.iter().find(|r| r.table == "orders").unwrap();
        assert_eq!(referenced.schema, db.name);
        assert_eq!(referenced.name, "orders_customer_fk");
        assert_eq!(referenced.columns, vec!["customer_id".to_string()]);
        assert!(customers.ddl.starts_with("CREATE TABLE `customers`"), "{}", customers.ddl);
        assert!(customers.ddl.ends_with(';'));
        assert_eq!(customers.meta.kind, "table");
        assert_eq!(customers.meta.engine.as_deref(), Some("InnoDB"));
        assert!(customers.meta.collation.as_deref().unwrap_or("").starts_with("utf8mb4"));
        assert_eq!(customers.meta.comment.as_deref(), Some("People who buy things"));
        assert!(customers.meta.table_bytes.unwrap() > 0);
        assert!(customers.meta.indexes_bytes.is_some());
        assert_eq!(customers.columns.len(), 5);
        assert!(customers.triggers.is_empty());

        let orders = catalog.table_structure(&db.name, "orders").await.unwrap();
        assert!(orders.warnings.is_empty(), "{:?}", orders.warnings);
        assert_eq!(orders.foreign_keys.len(), 1);
        let fk = &orders.foreign_keys[0];
        assert_eq!(fk.name, "orders_customer_fk");
        assert_eq!(fk.columns, vec!["customer_id".to_string()]);
        assert_eq!((fk.ref_schema.as_str(), fk.ref_table.as_str()), (db.name.as_str(), "customers"));
        assert_eq!(fk.ref_columns, vec!["id".to_string()]);
        assert_eq!(fk.on_delete, "CASCADE");
        assert_eq!(fk.on_update, "NO ACTION");
        let fk_constraint = orders.constraints.iter().find(|c| c.name == "orders_customer_fk").unwrap();
        assert_eq!(fk_constraint.kind, "foreign key");
        assert!(fk_constraint.definition.starts_with("FOREIGN KEY (`customer_id`) REFERENCES `"), "{}", fk_constraint.definition);
        assert!(fk_constraint.definition.ends_with("ON DELETE CASCADE"), "{}", fk_constraint.definition);
        let composite = orders.indexes.iter().find(|i| i.name == "orders_customer_status_idx").unwrap();
        assert_eq!(composite.columns, vec!["customer_id".to_string(), "status".to_string()]);
        assert!(!composite.is_unique);
        assert_eq!(orders.triggers.len(), 1);
        let trigger = &orders.triggers[0];
        assert_eq!(trigger.name, "orders_bi");
        assert_eq!(trigger.timing, "BEFORE");
        assert_eq!(trigger.events, vec!["INSERT".to_string()]);
        assert_eq!(trigger.level, "ROW");
        assert!(trigger.enabled);
        assert!(trigger.definition.starts_with("CREATE"), "{}", trigger.definition);
        assert!(trigger.definition.contains("TRIGGER `orders_bi`"), "{}", trigger.definition);
        assert!(orders.ddl.contains("ON UPDATE CURRENT_TIMESTAMP"), "{}", orders.ddl);

        // A view: no keys or indexes, its own DDL
        let view = catalog.table_structure(&db.name, "big_orders").await.unwrap();
        assert_eq!(view.meta.kind, "view");
        assert_eq!(view.meta.engine, None);
        assert_eq!(view.meta.comment, None);
        assert_eq!(view.meta.row_estimate, None);
        assert_eq!(view.columns.len(), 3);
        assert!(view.indexes.is_empty() && view.constraints.is_empty() && view.triggers.is_empty());
        assert!(view.ddl.starts_with("CREATE"), "{}", view.ddl);
        assert!(view.ddl.contains("VIEW `big_orders`"), "{}", view.ddl);

        let missing = catalog.table_structure(&db.name, "nope").await.err().unwrap();
        assert_eq!(missing, format!("{}.nope doesn't exist", db.name));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn table_privileges_are_listed_by_grantee() {
        let (db, driver) = shop().await;
        // A user that holds a privilege on the table: the structure lists it
        let user = format!("u_{}", &db.name[9..]);
        exec(
            &db.endpoint,
            &format!(
                "CREATE USER '{user}'@'%' IDENTIFIED BY 'pw'; \
                 GRANT SELECT, UPDATE ON `{}`.`customers` TO '{user}'@'%' WITH GRANT OPTION",
                db.name
            ),
        )
        .await;
        let structure = driver.catalog().table_structure(&db.name, "customers").await.unwrap();
        let privilege = structure.privileges.iter().find(|p| p.grantee.contains(&user)).expect("the user's grant");
        assert_eq!(privilege.privileges, vec!["SELECT".to_string(), "UPDATE".to_string()]);
        assert_eq!(privilege.grantable, vec!["SELECT".to_string(), "UPDATE".to_string()]);
        assert!(structure.warnings.is_empty(), "{:?}", structure.warnings);
        exec(&db.endpoint, &format!("DROP USER '{user}'@'%'")).await;
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn definitions_come_from_show_create() {
        let (db, driver) = shop().await;
        let catalog = driver.catalog();

        let view = catalog.object_definition(&db.name, "big_orders", "view").await.unwrap();
        assert_eq!(view.kind, "view");
        assert!(view.ddl.contains("VIEW `big_orders`"), "{}", view.ddl);
        assert!(view.ddl.contains("`total` >= 100"), "{}", view.ddl);
        assert!(view.facts.iter().any(|(k, _)| k == "Definer"));

        let procedure = catalog.object_definition(&db.name, "touch_customer", "procedure").await.unwrap();
        assert_eq!(procedure.kind, "procedure");
        assert!(procedure.ddl.contains("PROCEDURE `touch_customer`"), "{}", procedure.ddl);
        assert!(procedure.ddl.contains("UPDATE customers SET bio = bio"), "{}", procedure.ddl);
        assert!(procedure.ddl.contains("END"), "{}", procedure.ddl);

        let function = catalog.object_definition(&db.name, "order_total", "function").await.unwrap();
        assert!(function.ddl.contains("FUNCTION `order_total`"), "{}", function.ddl);
        assert!(function.ddl.contains("RETURN IFNULL(t, 0)"), "{}", function.ddl);

        let trigger = catalog.object_definition(&db.name, "orders_bi", "trigger").await.unwrap();
        assert!(trigger.ddl.contains("TRIGGER `orders_bi`"), "{}", trigger.ddl);
        assert!(trigger.facts.contains(&("Table".to_string(), "orders".to_string())));

        let table = catalog.object_definition(&db.name, "customers", "table").await.unwrap();
        assert!(table.ddl.starts_with("CREATE TABLE `customers`"), "{}", table.ddl);

        let missing = catalog.object_definition(&db.name, "nope", "view").await.err().unwrap();
        assert_eq!(missing, format!("{}.nope doesn't exist", db.name));
        let unknown = catalog.object_definition(&db.name, "orders", "sequence").await.err().unwrap();
        assert_eq!(unknown, "Unknown object kind \"sequence\"");
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn routine_detail_lists_its_parameters() {
        let (db, driver) = shop().await;
        let catalog = driver.catalog();
        let function = catalog.function_detail(&db.name, "order_total", "o int").await.unwrap();
        assert_eq!(function.name, "order_total");
        assert_eq!(function.schema, db.name);
        assert_eq!(function.argument_types, "o int");
        assert_eq!(function.return_type, "decimal(10,2)");
        assert_eq!(function.kind, "function");
        assert_eq!(function.language, "SQL");
        assert!(function.source.contains("FUNCTION `order_total`"), "{}", function.source);
        assert!(function.owner.starts_with("root@"), "{}", function.owner);
        assert!(!function.is_strict);

        let procedure = catalog.function_detail(&db.name, "touch_customer", "IN id int").await.unwrap();
        assert_eq!(procedure.kind, "procedure");
        assert_eq!(procedure.argument_types, "IN id int");
        assert_eq!(procedure.return_type, "");

        let missing = catalog.function_detail(&db.name, "nope", "").await.err().unwrap();
        assert_eq!(missing, format!("{}.nope doesn't exist", db.name));
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn completion_and_diagram_cover_the_database() {
        let (db, driver) = shop().await;
        // A table of another database that points at a table of this one
        let other = TestDb::new().await;
        exec(
            &other.endpoint,
            &format!("CREATE TABLE visits (id INT PRIMARY KEY, customer INT, FOREIGN KEY (customer) REFERENCES `{}`.`customers` (id))", db.name),
        )
        .await;
        let catalog = driver.catalog();

        let completion = catalog.completion_schema().await.unwrap();
        assert_eq!(completion.current_schema.as_deref(), Some(db.name.as_str()));
        let relation = |name: &str| completion.relations.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no relation {name}"));
        assert_eq!(completion.relations.len(), 3, "the other database stays out");
        assert!(completion.relations.iter().all(|r| r.schema == db.name));
        assert_eq!(relation("customers").relation_type, "table");
        assert_eq!(relation("big_orders").relation_type, "view");
        let customers: Vec<(&str, &str)> = relation("customers").columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str())).collect();
        assert_eq!(customers, [("id", "int"), ("email", "varchar(120)"), ("name", "varchar(80)"), ("age", "int"), ("bio", "text")]);
        assert_eq!(relation("orders").columns.len(), 8);
        let functions: Vec<(&str, &str, &str)> =
            completion.functions.iter().map(|f| (f.name.as_str(), f.argument_types.as_str(), f.return_type.as_str())).collect();
        assert_eq!(functions, [("order_total", "o int", "decimal(10,2)"), ("touch_customer", "IN id int", "")]);

        let diagram = catalog.diagram(&db.name).await.unwrap();
        let table = |schema: &str, name: &str| diagram.tables.iter().find(|t| t.schema == schema && t.name == name);
        assert!(table(&db.name, "customers").is_some() && table(&db.name, "orders").is_some());
        assert!(table(&db.name, "big_orders").is_none(), "views aren't on the diagram");
        let relation = diagram.relations.iter().find(|r| r.name == "orders_customer_fk").unwrap();
        assert_eq!((relation.from_schema.as_str(), relation.from_table.as_str()), (db.name.as_str(), "orders"));
        assert_eq!((relation.to_schema.as_str(), relation.to_table.as_str()), (db.name.as_str(), "customers"));
        assert_eq!(relation.from_columns, vec!["customer_id".to_string()]);
        assert_eq!(relation.to_columns, vec!["id".to_string()]);
        let orders = table(&db.name, "orders").unwrap();
        assert_eq!(orders.columns.len(), 8);
        let diagram_column = |name: &str| orders.columns.iter().find(|c| c.name == name).unwrap();
        assert!(diagram_column("id").is_primary_key && !diagram_column("id").is_foreign_key);
        assert!(diagram_column("customer_id").is_foreign_key);
        assert_eq!(diagram_column("customer_id").type_name, "int");
        assert_eq!(diagram_column("customer_id").ordinal, 2);
        let customers = table(&db.name, "customers").unwrap();
        assert!(customers.columns.iter().find(|c| c.name == "id").unwrap().is_foreign_key, "the referenced end counts too");

        // The key of the other database reaches this one, and its table comes along
        assert!(table(&other.name, "visits").is_some());
        assert!(diagram.relations.iter().any(|r| r.from_schema == other.name && r.to_schema == db.name && r.to_table == "customers"));
        assert_eq!(diagram.tables.len(), 3);
        // From the other database's side, the shop's table it reaches comes along
        let from_other = catalog.diagram(&other.name).await.unwrap();
        assert!(from_other.tables.iter().any(|t| t.schema == db.name && t.name == "customers"));
        assert!(!from_other.tables.iter().any(|t| t.name == "orders"));
        driver.close().await;
        other.drop_now().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn the_helpers_of_the_editors_read_what_mysql_stores() {
        let (db, driver) = shop().await;
        exec(&db.endpoint, "CREATE TABLE logs (id INT) ENGINE = MyISAM").await;
        let mut conn = driver.conn().await.unwrap();

        let columns = column_definitions(&mut conn, &db.name, "orders").await.unwrap();
        let named = |name: &str| columns.iter().find(|c| c.name == name).unwrap();
        assert_eq!(columns.len(), 8);
        assert_eq!(named("id").extra, "auto_increment");
        assert_eq!(named("id").column_type, "int");
        assert!(!named("id").nullable && named("total_with_tax").nullable);
        assert_eq!(named("status").charset.as_deref(), Some("utf8mb4"));
        assert!(named("status").collation.as_deref().unwrap().starts_with("utf8mb4"));
        assert_eq!(named("total").charset, None);
        assert_eq!(named("total").default.as_deref(), Some("0.00"));
        assert!(named("total_with_tax").extra.contains("STORED GENERATED"), "{}", named("total_with_tax").extra);
        assert!(named("total_with_tax").generation_expression.as_deref().unwrap().contains("tax_rate"));
        assert_eq!(named("total").generation_expression, None);
        assert!(named("updated_at").extra.contains("on update CURRENT_TIMESTAMP"), "{}", named("updated_at").extra);
        assert!(named("placed_at").extra.contains("DEFAULT_GENERATED"), "{}", named("placed_at").extra);
        assert_eq!(named("placed_at").default.as_deref(), Some("CURRENT_TIMESTAMP"));
        let customers = column_definitions(&mut conn, &db.name, "customers").await.unwrap();
        assert_eq!(customers.iter().find(|c| c.name == "bio").unwrap().comment, "");
        assert!(column_definitions(&mut conn, &db.name, "nope").await.unwrap().is_empty());

        assert_eq!(table_engine(&mut conn, &db.name, "orders").await.unwrap().as_deref(), Some("InnoDB"));
        assert_eq!(table_engine(&mut conn, &db.name, "logs").await.unwrap().as_deref(), Some("MyISAM"));
        assert_eq!(table_engine(&mut conn, &db.name, "big_orders").await.unwrap(), None);
        assert_eq!(table_engine(&mut conn, &db.name, "nope").await.unwrap(), None);

        let kinds = constraint_kinds(&mut conn, &db.name, "customers").await.unwrap();
        let expected = [("PRIMARY", "PRIMARY KEY"), ("customers_email_uq", "UNIQUE"), ("customers_age_ck", "CHECK")];
        assert_eq!(kinds, expected.iter().map(|(n, k)| (n.to_string(), k.to_string())).collect());
        let kinds = constraint_kinds(&mut conn, &db.name, "orders").await.unwrap();
        assert_eq!(kinds.get("orders_customer_fk").map(String::as_str), Some("FOREIGN KEY"));
        drop(conn);
        driver.close().await;
        db.drop_now().await;
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn extensions_are_unsupported() {
        let db = TestDb::new().await;
        let driver = MysqlDriver::open(db.endpoint.clone(), None).await.unwrap();
        assert_eq!(driver.catalog().extensions().await.err().unwrap(), "Extensions isn't supported by MySQL");
        driver.close().await;
        db.drop_now().await;
    }
}
