//! Structure changes from the Structure tab and the table builder: columns, indexes,
//! constraints and permissions. Names are quoted; types, defaults and expressions are SQL the
//! user writes, so they're checked to be a single fragment (no `;`) and the generated SQL is
//! shown before it runs.
//!
//! MySQL changes a column's type, nullability or comment only by `MODIFY`, which restates the
//! whole column, so those changes start from the column as the server keeps it.

use std::collections::HashMap;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;

use super::catalog::{column_definitions, constraint_kinds, qualified, quote, ColumnDefinition};
use super::objects::{drop_sql, duplicate, rename_sql, truncate_sql};
use crate::db::mysql_common::version::ServerVersion;
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::model::{ColumnGeneration, NewColumn, NewTable, StructureChange};
use crate::db::mysql_common::errors::error_message;
use crate::db::{unsupported, Engine, SchemaEditor};

/// Table privileges that can be granted or revoked.
const TABLE_PRIVILEGES: &[&str] =
    &["SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "REFERENCES", "INDEX", "ALTER", "CREATE VIEW", "SHOW VIEW", "TRIGGER"];

/// The data types whose columns have a character set.
const TEXT_TYPES: &[&str] = &["char", "varchar", "tinytext", "text", "mediumtext", "longtext", "enum", "set"];

/// How long a MySQL identifier may be, in characters.
const MAX_IDENTIFIER: usize = 64;

/// A string literal: backslashes and quotes are escaped the way the server's default mode reads them.
fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "''"))
}

fn fragment(label: &str, sql: &str) -> Result<String, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(format!("The {label} can't be empty"));
    }
    if trimmed.contains(';') {
        return Err(format!("The {label} must be a single expression, without ';'"));
    }
    Ok(trimmed.to_string())
}

fn name(label: &str, value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err(format!("The {label} can't be empty"));
    }
    Ok(quote(value.trim()))
}

fn column_list(label: &str, columns: &[String]) -> Result<String, String> {
    if columns.is_empty() {
        return Err(format!("Choose at least one {label}"));
    }
    Ok(columns.iter().map(|c| name(label, c)).collect::<Result<Vec<_>, _>>()?.join(", "))
}

/// `CONSTRAINT `name` ` when one was given, nothing to let the server pick it.
fn constraint_name(value: &Option<String>) -> Result<String, String> {
    match value.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => Ok(format!("CONSTRAINT {} ", name("constraint name", n)?)),
        None => Ok(String::new()),
    }
}

fn referential_action(label: &str, action: &str) -> Result<&'static str, String> {
    match action.trim().to_uppercase().as_str() {
        "" | "NO ACTION" => Ok("NO ACTION"),
        "RESTRICT" => Ok("RESTRICT"),
        "CASCADE" => Ok("CASCADE"),
        "SET NULL" => Ok("SET NULL"),
        "SET DEFAULT" => Ok("SET DEFAULT"),
        other => Err(format!("Unknown {label} action \"{other}\"")),
    }
}

/// Whether the first `(` of `text` closes at its last character: `(a + b)` yes, `(a) + (b)` no.
fn fully_parenthesized(text: &str) -> bool {
    let chars: Vec<char> = text.trim().chars().collect();
    if chars.first() != Some(&'(') || chars.last() != Some(&')') {
        return false;
    }
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            quote @ ('\'' | '"' | '`') => {
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    i += if chars[i] == '\\' && quote != '`' { 2 } else { 1 };
                }
            }
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i == chars.len() - 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// An expression in parentheses, unless it already is.
fn parenthesized(expression: &str) -> String {
    if fully_parenthesized(expression) {
        expression.trim().to_string()
    } else {
        format!("({})", expression.trim())
    }
}

/// `CURRENT_TIMESTAMP` or `CURRENT_TIMESTAMP(3)`: a default that needs no parentheses.
fn is_current_timestamp(default: &str) -> bool {
    let upper = default.trim().to_uppercase();
    match upper.strip_prefix("CURRENT_TIMESTAMP") {
        Some("") => true,
        Some(rest) => rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')).is_some_and(|digits| digits.chars().all(|c| c.is_ascii_digit())),
        None => false,
    }
}

/// What a column's `EXTRA` says, apart from the words the definition spells out by itself.
struct Extra {
    /// `DEFAULT_GENERATED`: the default is an expression
    default_is_expression: bool,
    /// `STORED GENERATED` or `VIRTUAL GENERATED`: Some(stored)
    generated: Option<bool>,
    /// What is left: `auto_increment`, `ON UPDATE CURRENT_TIMESTAMP`, `INVISIBLE`...
    rest: String,
}

fn read_extra(extra: &str) -> Extra {
    let mut rest = extra.to_string();
    let mut take = |word: &str| -> bool {
        match rest.to_lowercase().find(&word.to_lowercase()) {
            Some(at) => {
                rest.replace_range(at..at + word.len(), "");
                true
            }
            None => false,
        }
    };
    let default_is_expression = take("DEFAULT_GENERATED");
    let stored = take("STORED GENERATED");
    let virtual_ = take("VIRTUAL GENERATED");
    let mut rest = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    if rest.to_lowercase().starts_with("on update") {
        rest.replace_range(.."on update".len(), "ON UPDATE");
    }
    Extra { default_is_expression, generated: (stored || virtual_).then_some(stored), rest }
}

/// The whole column after its name, the way `MODIFY COLUMN` wants it. A column is restated with
/// everything the server keeps about it: character set and collation (text types only), whether
/// it is generated, null, its default, auto increment, `ON UPDATE`, visibility and comment.
pub(crate) fn definition_sql(column: &ColumnDefinition) -> String {
    let extra = read_extra(&column.extra);
    let mut parts = vec![column.column_type.clone()];
    if TEXT_TYPES.contains(&column.data_type.to_lowercase().as_str()) && !column.inherits_collation {
        if let Some(charset) = column.charset.as_deref().filter(|c| !c.is_empty()) {
            parts.push(format!("CHARACTER SET {charset}"));
        }
        if let Some(collation) = column.collation.as_deref().filter(|c| !c.is_empty()) {
            parts.push(format!("COLLATE {collation}"));
        }
    }
    if let Some(expression) = column.generation_expression.as_deref() {
        // The server keeps the quotes inside the expression escaped
        let expression = expression.replace("\\'", "'");
        parts.push(format!("GENERATED ALWAYS AS {} {}", parenthesized(&expression), if extra.generated == Some(true) { "STORED" } else { "VIRTUAL" }));
        if !column.nullable {
            parts.push("NOT NULL".into());
        }
    } else {
        parts.push(if column.nullable { "NULL" } else { "NOT NULL" }.into());
        if let Some(default) = column.default.as_deref() {
            parts.push(format!("DEFAULT {}", default_sql(column, default, extra.default_is_expression)));
        }
        if !extra.rest.is_empty() {
            parts.push(extra.rest);
        }
    }
    if !column.comment.is_empty() {
        parts.push(format!("COMMENT {}", literal(&column.comment)));
    }
    parts.join(" ")
}

fn default_sql(column: &ColumnDefinition, default: &str, is_expression: bool) -> String {
    if is_expression {
        if is_current_timestamp(default) {
            default.trim().to_string()
        } else {
            parenthesized(default)
        }
    } else if column.data_type.eq_ignore_ascii_case("bit") && default.starts_with("b'") {
        default.to_string()
    } else {
        literal(default)
    }
}

/// `{table}_{columns}_idx`, short enough for MySQL. Columns that are expressions give their words.
pub(crate) fn index_name(table: &str, columns: &[String]) -> String {
    let words: Vec<String> = columns
        .iter()
        .map(|c| {
            let spelled: String = c.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
            spelled.split('_').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("_")
        })
        .filter(|w| !w.is_empty())
        .collect();
    let base: String = format!("{table}_{}", words.join("_")).chars().take(MAX_IDENTIFIER - "_idx".len()).collect();
    format!("{base}_idx")
}

/// The table's own name out of `` `schema`.`table` ``.
fn plain_table(qualified: &str) -> String {
    let table = qualified.rsplit_once("`.`").map_or(qualified, |(_, table)| table);
    table.trim_matches('`').replace("``", "`")
}

/// An index key part: a plain column name is quoted, anything else is an expression in parentheses.
fn index_key_part(column: &str) -> Result<String, String> {
    let c = column.trim();
    if !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        Ok(quote(c))
    } else {
        Ok(format!("({})", fragment("index expression", c)?))
    }
}

/// An account as `user@host` (the host `%` when none): `'user'@'host'`.
fn account(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("The account can't be empty".into());
    }
    let (user, host) = value.rsplit_once('@').unwrap_or((value, "%"));
    if user.is_empty() {
        return Err("The account can't be empty".into());
    }
    Ok(format!("{}@{}", literal(user), literal(if host.is_empty() { "%" } else { host })))
}

fn privileges(list: &[String]) -> Result<String, String> {
    if list.is_empty() {
        return Err("Choose at least one privilege".into());
    }
    let upper = list
        .iter()
        .map(|p| {
            let p = p.trim().to_uppercase();
            if TABLE_PRIVILEGES.contains(&p.as_str()) {
                Ok(p)
            } else {
                Err(format!("Unknown privilege \"{p}\""))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(upper.join(", "))
}

fn no_identity() -> String {
    "MySQL has no identity columns; use auto increment.".into()
}

/// A new column after its name: type, null, default, auto increment or generation, `UNIQUE`,
/// `CHECK` and comment. A primary key column of a new table is `NOT NULL` already.
fn column_body(column: &NewColumn, primary_key_implies_not_null: bool) -> Result<String, String> {
    let mut line = fragment("type", &column.data_type)?;
    let default = column.default.as_deref().filter(|d| !d.trim().is_empty());
    let not_null = if column.nullable { "NULL" } else { "NOT NULL" };
    let skip_null = primary_key_implies_not_null && column.primary_key && !column.nullable;
    match column.generation {
        ColumnGeneration::None => {
            if !skip_null {
                line += &format!(" {not_null}");
            }
            if let Some(default) = default {
                line += &format!(" DEFAULT {}", fragment("default", default)?);
            }
        }
        ColumnGeneration::Autoincrement => {
            if default.is_some() {
                return Err("An auto increment column can't have a default".into());
            }
            if !skip_null {
                line += &format!(" {not_null}");
            }
            line += " AUTO_INCREMENT";
        }
        ColumnGeneration::Generated => {
            if default.is_some() {
                return Err("A generated column can't have a default".into());
            }
            let expression = column.generated.as_deref().filter(|e| !e.trim().is_empty()).ok_or("The generated expression can't be empty")?;
            line += &format!(" GENERATED ALWAYS AS {} STORED", parenthesized(&fragment("generated expression", expression)?));
            if !column.nullable && !skip_null {
                line += " NOT NULL";
            }
        }
        ColumnGeneration::IdentityByDefault | ColumnGeneration::IdentityAlways => return Err(no_identity()),
    }
    if column.unique {
        line += " UNIQUE";
    }
    if let Some(check) = column.check.as_deref().filter(|c| !c.trim().is_empty()) {
        line += &format!(" CHECK ({})", fragment("check", check)?);
    }
    if let Some(comment) = column.comment.as_deref().filter(|c| !c.trim().is_empty()) {
        line += &format!(" COMMENT {}", literal(comment));
    }
    Ok(line)
}

/// `ALTER TABLE … MODIFY COLUMN` for `name` as it is now, after `update` changed what it must.
fn modify(table: &str, columns: &[ColumnDefinition], name: &str, update: impl FnOnce(&mut ColumnDefinition) -> Result<(), String>) -> Result<String, String> {
    let wanted = name.trim();
    let mut column = columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(wanted))
        .cloned()
        .ok_or_else(|| format!("Column {wanted} doesn't exist"))?;
    update(&mut column)?;
    Ok(format!("ALTER TABLE {table} MODIFY COLUMN {} {}", quote(&column.name), definition_sql(&column)))
}

/// The statement a change runs. `table` is `` `schema`.`table` ``; `columns` and `constraints`
/// are what the table has now, read only for the changes that restate a column or drop a constraint.
pub(crate) fn change_sql(
    table: &str,
    change: &StructureChange,
    columns: &[ColumnDefinition],
    constraints: &HashMap<String, String>,
    version: ServerVersion,
) -> Result<String, String> {
    let alter = format!("ALTER TABLE {table}");
    Ok(match change {
        StructureChange::AddColumn { name: column, data_type, nullable, default, comment, unique, check, generation, generated } => {
            let spec = NewColumn {
                name: column.clone(),
                data_type: data_type.clone(),
                nullable: *nullable,
                primary_key: false,
                default: default.clone(),
                unique: *unique,
                check: check.clone(),
                comment: comment.clone(),
                generation: *generation,
                generated: generated.clone(),
            };
            format!("{alter} ADD COLUMN {} {}", name("column name", column)?, column_body(&spec, false)?)
        }
        StructureChange::RenameColumn { name: column, new_name } => {
            format!("{alter} RENAME COLUMN {} TO {}", name("column name", column)?, name("new column name", new_name)?)
        }
        StructureChange::DropColumn { name: column, .. } => format!("{alter} DROP COLUMN {}", name("column name", column)?),
        StructureChange::SetDefault { name: column, default } => match default.as_deref().filter(|d| !d.trim().is_empty()) {
            Some(default) => format!("{alter} ALTER COLUMN {} SET DEFAULT {}", name("column name", column)?, fragment("default", default)?),
            None => format!("{alter} ALTER COLUMN {} DROP DEFAULT", name("column name", column)?),
        },
        StructureChange::SetNullable { name: column, nullable } => modify(table, columns, column, |c| {
            c.nullable = *nullable;
            Ok(())
        })?,
        StructureChange::SetComment { name: column, comment } => modify(table, columns, column, |c| {
            c.comment = comment.clone().unwrap_or_default();
            Ok(())
        })?,
        StructureChange::SetType { name: column, data_type, using } => {
            if using.as_deref().is_some_and(|u| !u.trim().is_empty()) {
                return Err("MySQL has no USING clause; convert the values first.".into());
            }
            let column_type = fragment("type", data_type)?;
            modify(table, columns, column, |c| {
                c.data_type = column_type.split(|ch: char| ch == '(' || ch.is_whitespace()).next().unwrap_or("").to_lowercase();
                c.column_type = column_type;
                Ok(())
            })?
        }
        StructureChange::CreateIndex { name: index, columns: key_columns, unique, method, concurrently } => {
            if *concurrently {
                return Err(unsupported(Engine::Mysql, "Creating indexes concurrently"));
            }
            if key_columns.is_empty() {
                return Err("Choose at least one column".into());
            }
            let (kind, using) = match method.trim().to_uppercase().as_str() {
                "" => ("", ""),
                "BTREE" => ("", " USING BTREE"),
                "HASH" => ("", " USING HASH"),
                "FULLTEXT" => ("FULLTEXT ", ""),
                "SPATIAL" => ("SPATIAL ", ""),
                other => return Err(format!("Unknown index method \"{}\"", other.to_lowercase())),
            };
            if *unique && !kind.is_empty() {
                return Err(format!("A {} index can't be unique", kind.trim()));
            }
            let index = match index.as_deref().filter(|n| !n.trim().is_empty()) {
                Some(given) => name("index name", given)?,
                None => quote(&index_name(&plain_table(table), key_columns)),
            };
            let parts = key_columns.iter().map(|c| index_key_part(c)).collect::<Result<Vec<_>, _>>()?;
            format!("CREATE {}{kind}INDEX {index} ON {table} ({}){using}", if *unique { "UNIQUE " } else { "" }, parts.join(", "))
        }
        StructureChange::DropIndex { name: index, .. } => {
            if index == "PRIMARY" {
                format!("{alter} DROP PRIMARY KEY")
            } else {
                format!("DROP INDEX {} ON {table}", name("index name", index)?)
            }
        }
        StructureChange::AddForeignKey { name: constraint, columns: from, ref_schema, ref_table, ref_columns, on_update, on_delete } => {
            if from.len() != ref_columns.len() {
                return Err("Pick as many referenced columns as columns".into());
            }
            format!(
                "{alter} ADD {}FOREIGN KEY ({}) REFERENCES {}.{} ({}) ON UPDATE {} ON DELETE {}",
                constraint_name(constraint)?,
                column_list("column", from)?,
                name("referenced schema", ref_schema)?,
                name("referenced table", ref_table)?,
                column_list("referenced column", ref_columns)?,
                referential_action("update", on_update)?,
                referential_action("delete", on_delete)?,
            )
        }
        StructureChange::AddUnique { name: constraint, columns: key_columns } => {
            format!("{alter} ADD {}UNIQUE ({})", constraint_name(constraint)?, column_list("column", key_columns)?)
        }
        StructureChange::AddCheck { name: constraint, expression } => {
            if !version.at_least(8, 0, 16) {
                return Err("CHECK constraints need MySQL 8.0.16 or newer".into());
            }
            format!("{alter} ADD {}CHECK ({})", constraint_name(constraint)?, fragment("check", expression)?)
        }
        StructureChange::DropConstraint { name: constraint, .. } => {
            let given = constraint.trim();
            // The kind decides the clause: only 8.0.19 and later have a plain DROP CONSTRAINT
            match constraints.get(given).map(String::as_str) {
                Some("FOREIGN KEY") => format!("{alter} DROP FOREIGN KEY {}", quote(given)),
                Some("CHECK") => format!("{alter} DROP CHECK {}", quote(given)),
                Some("UNIQUE") => format!("{alter} DROP INDEX {}", quote(given)),
                Some("PRIMARY KEY") => format!("{alter} DROP PRIMARY KEY"),
                _ => return Err(format!("Constraint {given} doesn't exist")),
            }
        }
        StructureChange::SetTriggerEnabled { .. } => return Err(unsupported(Engine::Mysql, "Turning triggers on and off")),
        StructureChange::Grant { grantee, privileges: list, grant_option } => {
            let option = if *grant_option { " WITH GRANT OPTION" } else { "" };
            format!("GRANT {} ON {table} TO {}{option}", privileges(list)?, account(grantee)?)
        }
        StructureChange::Revoke { grantee, privileges: list } => format!("REVOKE {} ON {table} FROM {}", privileges(list)?, account(grantee)?),
    })
}

/// `CREATE TABLE` from the table builder. Several primary key columns make a composite key.
pub(crate) fn create_table_sql(table: &str, spec: &NewTable) -> Result<String, String> {
    if spec.unlogged {
        return Err(unsupported(Engine::Mysql, "Unlogged tables"));
    }
    if spec.without_rowid {
        return Err(unsupported(Engine::Mysql, "WITHOUT ROWID"));
    }
    if spec.strict {
        return Err(unsupported(Engine::Mysql, "STRICT tables"));
    }
    if spec.partition.is_some() {
        return Err(unsupported(Engine::Mysql, "Partitions"));
    }
    if spec.columns.is_empty() {
        return Err("Add at least one column".into());
    }
    let mut seen: Vec<String> = Vec::new();
    let mut lines = Vec::new();
    for column in &spec.columns {
        let quoted = name("column name", &column.name)?;
        if seen.contains(&column.name.trim().to_lowercase()) {
            return Err(format!("Column {} appears twice", column.name.trim()));
        }
        seen.push(column.name.trim().to_lowercase());
        lines.push(format!("{quoted} {}", column_body(column, true)?));
    }
    let keys: Vec<String> = spec.columns.iter().filter(|c| c.primary_key).map(|c| quote(c.name.trim())).collect();
    if !keys.is_empty() {
        lines.push(format!("PRIMARY KEY ({})", keys.join(", ")));
    }
    for key in &spec.foreign_keys {
        if key.columns.len() != key.ref_columns.len() {
            return Err("Pick as many referenced columns as columns".into());
        }
        lines.push(format!(
            "FOREIGN KEY ({}) REFERENCES {}.{} ({}) ON UPDATE {} ON DELETE {}",
            column_list("column", &key.columns)?,
            name("referenced schema", &key.ref_schema)?,
            name("referenced table", &key.ref_table)?,
            column_list("referenced column", &key.ref_columns)?,
            referential_action("update", &key.on_update)?,
            referential_action("delete", &key.on_delete)?,
        ));
    }
    Ok(format!("CREATE TABLE {table} (\n    {}\n)", lines.join(",\n    ")))
}

impl ServerDriver {
    /// The statement for a change, reading the table's columns or constraints when the change needs them.
    async fn change_statement(&self, schema: &str, table: &str, change: &StructureChange) -> Result<String, String> {
        let restates_a_column = matches!(change, StructureChange::SetNullable { .. } | StructureChange::SetType { .. } | StructureChange::SetComment { .. });
        let drops_a_constraint = matches!(change, StructureChange::DropConstraint { .. });
        let (mut columns, mut constraints) = (Vec::new(), HashMap::new());
        if restates_a_column || drops_a_constraint {
            let mut conn = self.conn().await?;
            if restates_a_column {
                columns = column_definitions(&mut conn, schema, table).await?;
            }
            if drops_a_constraint {
                constraints = constraint_kinds(&mut conn, schema, table).await?;
            }
        }
        change_sql(&qualified(schema, table), change, &columns, &constraints, self.version())
    }

    async fn run_ddl(&self, sql: String) -> Result<(), String> {
        let mut conn = self.conn().await?;
        conn.query_drop(sql).await.map_err(|e| error_message(&e, self.endpoint()))
    }
}

#[async_trait]
impl SchemaEditor for ServerDriver {
    /// The SQL a change would run, for showing before running it.
    async fn preview_change(&self, schema: &str, table: &str, change: &StructureChange) -> Result<String, String> {
        Ok(format!("{};", self.change_statement(schema, table, change).await?))
    }

    async fn apply_change(&self, schema: &str, table: &str, change: StructureChange) -> Result<(), String> {
        let sql = self.change_statement(schema, table, &change).await?;
        self.run_ddl(sql).await
    }

    async fn preview_create_table(&self, schema: &str, table: &str, spec: &NewTable) -> Result<String, String> {
        Ok(format!("{};", create_table_sql(&qualified(schema, &table_name(table)?), spec)?))
    }

    async fn create_table(&self, schema: &str, table: &str, spec: NewTable) -> Result<(), String> {
        let sql = create_table_sql(&qualified(schema, &table_name(table)?), &spec)?;
        self.run_ddl(sql).await
    }

    async fn rename_relation(&self, schema: &str, name: &str, kind: &str, new_name: &str) -> Result<(), String> {
        self.run_ddl(rename_sql(schema, name, kind, new_name)?).await
    }

    async fn drop_relation(&self, schema: &str, name: &str, kind: &str, cascade: bool) -> Result<(), String> {
        self.run_ddl(drop_sql(schema, name, kind, cascade)?).await
    }

    /// MySQL always starts the numbering again, so `restart_identity` has nothing to ask.
    async fn truncate_table(&self, schema: &str, name: &str, _restart_identity: bool, cascade: bool) -> Result<(), String> {
        self.run_ddl(truncate_sql(schema, name, cascade)?).await
    }

    async fn duplicate_table(&self, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String> {
        duplicate(self, schema, name, new_name, with_data).await
    }

    async fn refresh_materialized_view(&self, _schema: &str, _name: &str) -> Result<(), String> {
        Err(unsupported(Engine::Mysql, "Materialized views"))
    }
}

/// The new table's name, which can't be empty.
fn table_name(table: &str) -> Result<String, String> {
    if table.trim().is_empty() {
        return Err("The table name can't be empty".into());
    }
    Ok(table.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::model::{ColumnGeneration, NewColumn, NewForeignKey};

    const TABLE: &str = "`shop`.`customers`";
    const V84: ServerVersion = ServerVersion { major: 8, minor: 4, patch: 0 };

    fn column(name: &str, column_type: &str) -> ColumnDefinition {
        let data_type = column_type.split(|c: char| c == '(' || c == ' ').next().unwrap_or("").to_lowercase();
        ColumnDefinition {
            name: name.into(),
            column_type: column_type.into(),
            data_type,
            charset: None,
            collation: None,
            nullable: true,
            default: None,
            extra: String::new(),
            generation_expression: None,
            comment: String::new(),
            inherits_collation: false,
        }
    }

    fn text_column(name: &str, column_type: &str, nullable: bool) -> ColumnDefinition {
        ColumnDefinition { charset: Some("utf8mb4".into()), collation: Some("utf8mb4_0900_ai_ci".into()), nullable, ..column(name, column_type) }
    }

    fn sql_with(change: StructureChange, columns: &[ColumnDefinition], constraints: &[(&str, &str)], version: ServerVersion) -> String {
        let constraints = constraints.iter().map(|(n, k)| (n.to_string(), k.to_string())).collect();
        change_sql(TABLE, &change, columns, &constraints, version).unwrap_or_else(|e| format!("ERR {e}"))
    }

    fn sql(change: StructureChange) -> String {
        let columns = [text_column("email", "varchar(120)", false), text_column("name", "varchar(80)", true), column("age", "int")];
        sql_with(change, &columns, &[], V84)
    }

    fn add_column(name: &str, data_type: &str) -> StructureChange {
        StructureChange::AddColumn {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default: None,
            comment: None,
            unique: false,
            check: None,
            generation: ColumnGeneration::None,
            generated: None,
        }
    }

    #[test]
    fn definitions_keep_every_attribute() {
        let bin = ColumnDefinition {
            charset: Some("utf8mb4".into()),
            collation: Some("utf8mb4_bin".into()),
            nullable: false,
            default: Some("it's".into()),
            comment: "a'b".into(),
            ..column("c", "varchar(80)")
        };
        assert_eq!(
            definition_sql(&bin),
            "varchar(80) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL DEFAULT 'it''s' COMMENT 'a''b'"
        );
        // A backslash is doubled in a literal
        let slash = ColumnDefinition { default: Some("a\\b".into()), nullable: false, ..column("c", "varchar(10)") };
        assert_eq!(definition_sql(&slash), "varchar(10) NOT NULL DEFAULT 'a\\\\b'");

        let stamp = ColumnDefinition {
            nullable: false,
            default: Some("CURRENT_TIMESTAMP".into()),
            extra: "DEFAULT_GENERATED on update CURRENT_TIMESTAMP".into(),
            ..column("c", "timestamp")
        };
        assert_eq!(definition_sql(&stamp), "timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP");
        let fractional = ColumnDefinition { default: Some("CURRENT_TIMESTAMP(3)".into()), extra: "DEFAULT_GENERATED".into(), ..column("c", "datetime(3)") };
        assert_eq!(definition_sql(&fractional), "datetime(3) NULL DEFAULT CURRENT_TIMESTAMP(3)");

        // An expression default needs parentheses unless it already has them
        let expr = ColumnDefinition { default: Some("expr".into()), extra: "DEFAULT_GENERATED".into(), ..column("c", "int") };
        assert_eq!(definition_sql(&expr), "int NULL DEFAULT (expr)");
        let wrapped = ColumnDefinition { default: Some("(1 + 1)".into()), extra: "DEFAULT_GENERATED".into(), ..column("c", "int") };
        assert_eq!(definition_sql(&wrapped), "int NULL DEFAULT (1 + 1)");
        let call = ColumnDefinition { default: Some("uuid()".into()), extra: "DEFAULT_GENERATED".into(), ..column("c", "varchar(36)") };
        assert_eq!(definition_sql(&call), "varchar(36) NULL DEFAULT (uuid())");

        // Generated columns have no default; NULL is the default and is left out
        let stored = ColumnDefinition { generation_expression: Some("expr".into()), extra: "STORED GENERATED".into(), ..column("c", "decimal(10,2)") };
        assert_eq!(definition_sql(&stored), "decimal(10,2) GENERATED ALWAYS AS (expr) STORED");
        let virtual_not_null = ColumnDefinition {
            nullable: false,
            comment: "sum".into(),
            generation_expression: Some("(`a` * 2)".into()),
            extra: "VIRTUAL GENERATED".into(),
            ..column("c", "int")
        };
        assert_eq!(definition_sql(&virtual_not_null), "int GENERATED ALWAYS AS (`a` * 2) VIRTUAL NOT NULL COMMENT 'sum'");
        // The server keeps the quotes of a string inside the expression escaped
        let quoted = ColumnDefinition {
            charset: Some("latin1".into()),
            collation: Some("latin1_swedish_ci".into()),
            generation_expression: Some("concat(`b`,_latin1\\' \\')".into()),
            extra: "VIRTUAL GENERATED".into(),
            ..column("c", "varchar(40)")
        };
        assert_eq!(
            definition_sql(&quoted),
            "varchar(40) CHARACTER SET latin1 COLLATE latin1_swedish_ci GENERATED ALWAYS AS (concat(`b`,_latin1' ')) VIRTUAL"
        );

        // A column that takes the table's collation says nothing of it: stating it makes the
        // server show it from then on
        let inherited = ColumnDefinition { inherits_collation: true, ..text_column("c", "varchar(20)", true) };
        assert_eq!(definition_sql(&inherited), "varchar(20) NULL");

        // CHARACTER SET and COLLATE only belong to text types and enums
        let number = ColumnDefinition { charset: Some("utf8mb4".into()), collation: Some("utf8mb4_bin".into()), ..column("c", "int unsigned") };
        assert_eq!(definition_sql(&number), "int unsigned NULL");
        let enumeration = ColumnDefinition { default: Some("y".into()), ..text_column("c", "enum('x','y')", true) };
        assert_eq!(
            definition_sql(&enumeration),
            "enum('x','y') CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NULL DEFAULT 'y'"
        );

        let auto = ColumnDefinition { nullable: false, extra: "auto_increment".into(), ..column("id", "bigint") };
        assert_eq!(definition_sql(&auto), "bigint NOT NULL auto_increment");
        let hidden = ColumnDefinition { extra: "INVISIBLE".into(), ..column("h", "int") };
        assert_eq!(definition_sql(&hidden), "int NULL INVISIBLE");
        let bit = ColumnDefinition { default: Some("b'101'".into()), ..column("b", "bit(3)") };
        assert_eq!(definition_sql(&bit), "bit(3) NULL DEFAULT b'101'");
        let nothing = ColumnDefinition { nullable: false, ..column("n", "int") };
        assert_eq!(definition_sql(&nothing), "int NOT NULL");
    }

    #[test]
    fn column_changes_have_their_statements() {
        let alter = "ALTER TABLE `shop`.`customers`";
        assert_eq!(
            sql(StructureChange::AddColumn {
                name: "nick".into(),
                data_type: "varchar(40)".into(),
                nullable: false,
                default: Some("'x'".into()),
                comment: Some("it's".into()),
                unique: true,
                check: Some("char_length(`nick`) > 1".into()),
                generation: ColumnGeneration::None,
                generated: None,
            }),
            format!("{alter} ADD COLUMN `nick` varchar(40) NOT NULL DEFAULT 'x' UNIQUE CHECK (char_length(`nick`) > 1) COMMENT 'it''s'")
        );
        assert_eq!(
            sql(StructureChange::AddColumn {
                name: "id".into(),
                data_type: "bigint".into(),
                nullable: false,
                default: None,
                comment: None,
                unique: false,
                check: None,
                generation: ColumnGeneration::Autoincrement,
                generated: None,
            }),
            format!("{alter} ADD COLUMN `id` bigint NOT NULL AUTO_INCREMENT")
        );
        assert_eq!(
            sql(StructureChange::AddColumn {
                name: "twice".into(),
                data_type: "int".into(),
                nullable: false,
                default: None,
                comment: None,
                unique: false,
                check: None,
                generation: ColumnGeneration::Generated,
                generated: Some("age * 2".into()),
            }),
            format!("{alter} ADD COLUMN `twice` int GENERATED ALWAYS AS (age * 2) STORED NOT NULL")
        );
        assert_eq!(sql(StructureChange::RenameColumn { name: "a`b".into(), new_name: "c".into() }), format!("{alter} RENAME COLUMN `a``b` TO `c`"));
        assert_eq!(sql(StructureChange::DropColumn { name: "age".into(), cascade: false }), format!("{alter} DROP COLUMN `age`"));

        // Defaults are restated without the whole column
        assert_eq!(sql(StructureChange::SetDefault { name: "name".into(), default: Some("'anon'".into()) }), format!("{alter} ALTER COLUMN `name` SET DEFAULT 'anon'"));
        assert_eq!(sql(StructureChange::SetDefault { name: "name".into(), default: Some("  ".into()) }), format!("{alter} ALTER COLUMN `name` DROP DEFAULT"));
        assert_eq!(sql(StructureChange::SetDefault { name: "name".into(), default: None }), format!("{alter} ALTER COLUMN `name` DROP DEFAULT"));

        // The rest restates the column as the server keeps it, with the one thing changed
        assert_eq!(
            sql(StructureChange::SetNullable { name: "email".into(), nullable: true }),
            format!("{alter} MODIFY COLUMN `email` varchar(120) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NULL")
        );
        assert_eq!(
            sql(StructureChange::SetNullable { name: "NAME".into(), nullable: false }),
            format!("{alter} MODIFY COLUMN `name` varchar(80) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL")
        );
        assert_eq!(
            sql(StructureChange::SetComment { name: "name".into(), comment: Some("hi".into()) }),
            format!("{alter} MODIFY COLUMN `name` varchar(80) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NULL COMMENT 'hi'")
        );
        assert_eq!(
            sql(StructureChange::SetComment { name: "name".into(), comment: None }),
            format!("{alter} MODIFY COLUMN `name` varchar(80) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NULL")
        );
        assert_eq!(
            sql(StructureChange::SetType { name: "email".into(), data_type: "varchar(200)".into(), using: None }),
            format!("{alter} MODIFY COLUMN `email` varchar(200) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL")
        );
        // A type that isn't text drops the character set the old one had
        assert_eq!(
            sql(StructureChange::SetType { name: "email".into(), data_type: "int unsigned".into(), using: Some("  ".into()) }),
            format!("{alter} MODIFY COLUMN `email` int unsigned NOT NULL")
        );
        assert_eq!(sql(StructureChange::SetNullable { name: "ghost".into(), nullable: true }), "ERR Column ghost doesn't exist");
    }

    #[test]
    fn indexes_have_their_statements() {
        let on = "ON `shop`.`customers`";
        let index = |name: Option<&str>, columns: &[&str], unique: bool, method: &str| StructureChange::CreateIndex {
            name: name.map(String::from),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            unique,
            method: method.into(),
            concurrently: false,
        };
        assert_eq!(sql(index(None, &["name"], false, "")), format!("CREATE INDEX `customers_name_idx` {on} (`name`)"));
        assert_eq!(sql(index(None, &["name", "age"], true, "btree")), format!("CREATE UNIQUE INDEX `customers_name_age_idx` {on} (`name`, `age`) USING BTREE"));
        assert_eq!(sql(index(Some("by_name"), &["name"], false, "HASH")), format!("CREATE INDEX `by_name` {on} (`name`) USING HASH"));
        assert_eq!(sql(index(Some("bio_ft"), &["bio"], false, "fulltext")), format!("CREATE FULLTEXT INDEX `bio_ft` {on} (`bio`)"));
        assert_eq!(sql(index(Some("geo"), &["area"], false, "Spatial")), format!("CREATE SPATIAL INDEX `geo` {on} (`area`)"));
        // An expression is its own key part, in parentheses; the name is made from its words
        assert_eq!(sql(index(None, &["lower(`email`)"], false, "")), format!("CREATE INDEX `customers_lower_email_idx` {on} ((lower(`email`)))"));
        assert_eq!(sql(index(Some("lo"), &["lower(`email`)", "name"], false, "")), format!("CREATE INDEX `lo` {on} ((lower(`email`)), `name`)"));
        assert_eq!(sql(index(Some("x"), &["a;b c"], false, "")), "ERR The index expression must be a single expression, without ';'");
        assert_eq!(sql(index(Some("x"), &["name"], true, "fulltext")), "ERR A FULLTEXT index can't be unique");
        assert_eq!(sql(index(Some("x"), &["name"], false, "evil")), "ERR Unknown index method \"evil\"");
        assert_eq!(sql(index(Some("x"), &[], false, "")), "ERR Choose at least one column");
        assert_eq!(
            sql(StructureChange::CreateIndex { name: None, columns: vec!["name".into()], unique: false, method: "".into(), concurrently: true }),
            "ERR Creating indexes concurrently isn't supported by MySQL"
        );

        assert_eq!(sql(StructureChange::DropIndex { name: "by_name".into(), cascade: false }), format!("DROP INDEX `by_name` {on}"));
        assert_eq!(sql(StructureChange::DropIndex { name: "PRIMARY".into(), cascade: false }), "ALTER TABLE `shop`.`customers` DROP PRIMARY KEY");
    }

    #[test]
    fn constraints_have_their_statements() {
        let alter = "ALTER TABLE `shop`.`customers`";
        assert_eq!(
            sql(StructureChange::AddForeignKey {
                name: Some("fk".into()),
                columns: vec!["a".into(), "b".into()],
                ref_schema: "shop".into(),
                ref_table: "people".into(),
                ref_columns: vec!["x".into(), "y".into()],
                on_update: "".into(),
                on_delete: "cascade".into(),
            }),
            format!("{alter} ADD CONSTRAINT `fk` FOREIGN KEY (`a`, `b`) REFERENCES `shop`.`people` (`x`, `y`) ON UPDATE NO ACTION ON DELETE CASCADE")
        );
        assert_eq!(
            sql(StructureChange::AddForeignKey {
                name: None,
                columns: vec!["a".into()],
                ref_schema: "shop".into(),
                ref_table: "people".into(),
                ref_columns: vec!["x".into(), "y".into()],
                on_update: "restrict".into(),
                on_delete: "set null".into(),
            }),
            "ERR Pick as many referenced columns as columns"
        );
        assert_eq!(sql(StructureChange::AddUnique { name: None, columns: vec!["a".into(), "b".into()] }), format!("{alter} ADD UNIQUE (`a`, `b`)"));
        assert_eq!(sql(StructureChange::AddUnique { name: Some("u".into()), columns: vec!["a".into()] }), format!("{alter} ADD CONSTRAINT `u` UNIQUE (`a`)"));
        assert_eq!(sql(StructureChange::AddCheck { name: Some("c".into()), expression: "age >= 0".into() }), format!("{alter} ADD CONSTRAINT `c` CHECK (age >= 0)"));
        assert_eq!(sql(StructureChange::AddCheck { name: None, expression: "age >= 0; --".into() }), "ERR The check must be a single expression, without ';'");
        let old = ServerVersion { major: 8, minor: 0, patch: 15 };
        assert_eq!(
            sql_with(StructureChange::AddCheck { name: None, expression: "age >= 0".into() }, &[], &[], old),
            "ERR CHECK constraints need MySQL 8.0.16 or newer"
        );

        let kinds = [("fk", "FOREIGN KEY"), ("ck", "CHECK"), ("uq", "UNIQUE"), ("PRIMARY", "PRIMARY KEY")];
        let drop = |name: &str| sql_with(StructureChange::DropConstraint { name: name.into(), cascade: false }, &[], &kinds, V84);
        assert_eq!(drop("fk"), format!("{alter} DROP FOREIGN KEY `fk`"));
        assert_eq!(drop("ck"), format!("{alter} DROP CHECK `ck`"));
        assert_eq!(drop("uq"), format!("{alter} DROP INDEX `uq`"));
        assert_eq!(drop("PRIMARY"), format!("{alter} DROP PRIMARY KEY"));
        assert_eq!(drop("nope"), "ERR Constraint nope doesn't exist");
    }

    #[test]
    fn permissions_have_their_statements() {
        assert_eq!(
            sql(StructureChange::Grant { grantee: "u".into(), privileges: vec!["select".into(), "Update".into()], grant_option: true }),
            "GRANT SELECT, UPDATE ON `shop`.`customers` TO 'u'@'%' WITH GRANT OPTION"
        );
        assert_eq!(
            sql(StructureChange::Grant { grantee: "ana@local'host".into(), privileges: vec!["SHOW VIEW".into()], grant_option: false }),
            "GRANT SHOW VIEW ON `shop`.`customers` TO 'ana'@'local''host'"
        );
        assert_eq!(
            sql(StructureChange::Revoke { grantee: "u@%".into(), privileges: vec!["SELECT".into()] }),
            "REVOKE SELECT ON `shop`.`customers` FROM 'u'@'%'"
        );
        assert_eq!(sql(StructureChange::Revoke { grantee: "u".into(), privileges: vec![] }), "ERR Choose at least one privilege");
        assert_eq!(sql(StructureChange::Revoke { grantee: "u".into(), privileges: vec!["ROOT".into()] }), "ERR Unknown privilege \"ROOT\"");
        assert_eq!(sql(StructureChange::Grant { grantee: " ".into(), privileges: vec!["SELECT".into()], grant_option: false }), "ERR The account can't be empty");
    }

    #[test]
    fn unsupported_options_say_why() {
        assert_eq!(
            sql(StructureChange::SetType { name: "email".into(), data_type: "int".into(), using: Some("email::int".into()) }),
            "ERR MySQL has no USING clause; convert the values first."
        );
        let mut identity = add_column("id", "bigint");
        if let StructureChange::AddColumn { generation, .. } = &mut identity {
            *generation = ColumnGeneration::IdentityAlways;
        }
        assert_eq!(sql(identity), "ERR MySQL has no identity columns; use auto increment.");
        assert_eq!(
            sql(StructureChange::SetTriggerEnabled { name: "t".into(), enabled: false }),
            "ERR Turning triggers on and off isn't supported by MySQL"
        );
        for (spec, message) in [
            (NewTable { unlogged: true, ..one_column() }, "Unlogged tables isn't supported by MySQL"),
            (NewTable { without_rowid: true, ..one_column() }, "WITHOUT ROWID isn't supported by MySQL"),
            (NewTable { strict: true, ..one_column() }, "STRICT tables isn't supported by MySQL"),
            (NewTable { partition: Some(crate::db::model::NewPartition { method: "range".into(), expression: "id".into() }), ..one_column() }, "Partitions isn't supported by MySQL"),
        ] {
            assert_eq!(create_table_sql(TABLE, &spec).err().as_deref(), Some(message));
        }
        assert_eq!(create_table_sql(TABLE, &NewTable::default()).err().as_deref(), Some("Add at least one column"));
    }

    fn one_column() -> NewTable {
        NewTable {
            columns: vec![NewColumn {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                primary_key: true,
                default: None,
                unique: false,
                check: None,
                comment: None,
                generation: ColumnGeneration::None,
                generated: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_new_table_has_columns_keys_and_foreign_keys() {
        let spec = NewTable {
            columns: vec![
                NewColumn {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                    primary_key: true,
                    default: None,
                    unique: false,
                    check: None,
                    comment: None,
                    generation: ColumnGeneration::Autoincrement,
                    generated: None,
                },
                NewColumn {
                    name: "email".into(),
                    data_type: "varchar(120)".into(),
                    nullable: false,
                    primary_key: false,
                    default: Some("''".into()),
                    unique: true,
                    check: Some("char_length(`email`) > 0".into()),
                    comment: Some("it's".into()),
                    generation: ColumnGeneration::None,
                    generated: None,
                },
                NewColumn {
                    name: "owner".into(),
                    data_type: "bigint".into(),
                    nullable: true,
                    primary_key: false,
                    default: None,
                    unique: false,
                    check: None,
                    comment: None,
                    generation: ColumnGeneration::None,
                    generated: None,
                },
                NewColumn {
                    name: "twice".into(),
                    data_type: "bigint".into(),
                    nullable: true,
                    primary_key: false,
                    default: None,
                    unique: false,
                    check: None,
                    comment: None,
                    generation: ColumnGeneration::Generated,
                    generated: Some("id * 2".into()),
                },
            ],
            foreign_keys: vec![NewForeignKey {
                columns: vec!["owner".into()],
                ref_schema: "shop".into(),
                ref_table: "people".into(),
                ref_columns: vec!["id".into()],
                on_update: "".into(),
                on_delete: "cascade".into(),
            }],
            ..Default::default()
        };
        assert_eq!(
            create_table_sql("`shop`.`items`", &spec).unwrap(),
            "CREATE TABLE `shop`.`items` (\n    `id` bigint AUTO_INCREMENT,\n    `email` varchar(120) NOT NULL DEFAULT '' UNIQUE CHECK (char_length(`email`) > 0) COMMENT 'it''s',\n    `owner` bigint NULL,\n    `twice` bigint GENERATED ALWAYS AS (id * 2) STORED,\n    PRIMARY KEY (`id`),\n    FOREIGN KEY (`owner`) REFERENCES `shop`.`people` (`id`) ON UPDATE NO ACTION ON DELETE CASCADE\n)"
        );
        let mut twice = spec.clone();
        twice.columns.push(twice.columns[0].clone());
        assert_eq!(create_table_sql("`shop`.`items`", &twice).err().as_deref(), Some("Column id appears twice"));
        let mut identity = one_column();
        identity.columns[0].generation = ColumnGeneration::IdentityByDefault;
        assert_eq!(create_table_sql("`shop`.`items`", &identity).err().as_deref(), Some("MySQL has no identity columns; use auto increment."));
    }

    #[test]
    fn index_names_fit_in_64_characters() {
        assert_eq!(index_name("customers", &["name".into()]), "customers_name_idx");
        assert_eq!(index_name("t", &["lower(`email`)".into(), "a-b".into()]), "t_lower_email_a_b_idx");
        let long = index_name(&"t".repeat(50), &["first_column".into(), "second_column".into()]);
        assert!(long.chars().count() <= 64 && long.ends_with("_idx") && long.starts_with("tttt"), "{long}");
        let wide = index_name("tábua", &["ção".repeat(30)]);
        assert!(wide.chars().count() <= 64 && wide.ends_with("_idx"), "{wide}");
    }
}

#[cfg(test)]
mod server_tests {
    use std::sync::Arc;

    use mysql_async::prelude::Queryable;
    use mysql_async::Conn;

    use super::super::catalog::{column_definitions, qualified, quote};
    use crate::db::mysql_common::tests_support::{exec, query_one, TestDb};
    use crate::db::mysql_common::driver::ServerDriver;
    use super::*;
    use crate::db::model::{NewColumn, NewForeignKey, StructureChange};
    use crate::db::Driver;

    const SHOP: &str = include_str!("fixtures/shop.sql");

    /// A table with the columns that are hardest to restate: a character set and collation of
    /// their own, a default with quotes, `ON UPDATE`, expression defaults, stored and virtual
    /// generated columns, an invisible one, bit, JSON and comments with quotes.
    const HARD: &str = r#"
        CREATE TABLE hard (
            id INT NOT NULL AUTO_INCREMENT,
            a INT NOT NULL DEFAULT 5,
            b VARCHAR(20) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'it''s \\ x',
            c TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
            d INT DEFAULT (1 + 1),
            e VARCHAR(36) DEFAULT (uuid()),
            f DECIMAL(10,2) GENERATED ALWAYS AS (a * 2) STORED,
            g VARCHAR(40) CHARACTER SET latin1 GENERATED ALWAYS AS (concat(b, ' ', 'x')) VIRTUAL NOT NULL,
            h INT INVISIBLE,
            i BIT(3) DEFAULT b'101',
            j DATETIME(3) DEFAULT CURRENT_TIMESTAMP(3),
            k INT NULL DEFAULT NULL COMMENT 'it''s',
            l ENUM('x','y') DEFAULT 'y',
            m DATE DEFAULT (CURRENT_DATE),
            n JSON DEFAULT (JSON_ARRAY()),
            o TINYINT(1) DEFAULT 1,
            p DOUBLE DEFAULT 1.5,
            q TEXT,
            r VARCHAR(30) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin COMMENT 'quote '' and \\ slash',
            PRIMARY KEY (id)
        )"#;

    async fn open(setup: &str) -> (TestDb, Arc<ServerDriver>) {
        let db = TestDb::new().await;
        exec(&db.endpoint, setup).await;
        let driver = crate::db::mysql_common::tests_support::open_server(db.endpoint.clone()).await.unwrap();
        (db, driver)
    }

    async fn show_create(conn: &mut Conn, db: &str, table: &str) -> String {
        let row: (String, String) = conn.query_first(format!("SHOW CREATE TABLE {}", qualified(db, table))).await.unwrap().unwrap();
        row.1
    }

    /// Whether a row of `information_schema.{view}` names `thing` in `table`.
    async fn exists(db: &TestDb, view: &str, name_column: &str, table: &str, thing: &str) -> bool {
        let sql = format!(
            "SELECT COUNT(*) FROM information_schema.{view} WHERE table_schema = '{}' AND table_name = '{table}' AND {name_column} = '{thing}'",
            db.name
        );
        query_one::<i64>(&db.endpoint, &sql).await.unwrap() > 0
    }

    /// Drops the account when the test ends, also when it fails.
    struct Account(String);

    impl Drop for Account {
        fn drop(&mut self) {
            let sql = format!("DROP USER IF EXISTS {}", self.0);
            let _ = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                runtime.block_on(exec(&crate::db::mysql_common::tests_support::test_endpoint(), &sql));
            })
            .join();
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn modify_without_change_keeps_show_create_table() {
        let (db, d) = open(HARD).await;
        let mut conn = d.conn().await.unwrap();
        let before = show_create(&mut conn, &db.name, "hard").await;
        let columns = column_definitions(&mut conn, &db.name, "hard").await.unwrap();
        assert_eq!(columns.len(), 19);
        for column in &columns {
            let sql = format!("ALTER TABLE {} MODIFY COLUMN {} {}", qualified(&db.name, "hard"), quote(&column.name), definition_sql(column));
            conn.query_drop(&sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
            assert_eq!(show_create(&mut conn, &db.name, "hard").await, before, "restating `{}` changed the table: {sql}", column.name);
        }
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn every_change_applies_on_a_real_server() {
        let (db, d) = open(SHOP).await;
        let editor = d.schema_editor().expect("mysql edits structure");
        let schema = db.name.as_str();
        let apply = |change: StructureChange| {
            let editor = editor;
            async move {
                let preview = editor.preview_change(schema, "customers", &change).await.unwrap();
                assert!(preview.ends_with(';'), "{preview}");
                editor.apply_change(schema, "customers", change).await.unwrap_or_else(|e| panic!("{preview}: {e}"));
            }
        };
        let column = |name: &'static str| {
            let d = d.clone();
            let schema = schema.to_string();
            async move {
                let mut conn = d.conn().await.unwrap();
                column_definitions(&mut conn, &schema, "customers").await.unwrap().into_iter().find(|c| c.name == name)
            }
        };

        apply(StructureChange::AddColumn {
            name: "nick".into(),
            data_type: "varchar(40)".into(),
            nullable: false,
            default: Some("'x'".into()),
            comment: Some("it's".into()),
            unique: true,
            check: None,
            generation: crate::db::model::ColumnGeneration::None,
            generated: None,
        })
        .await;
        // A column used by a check can't be renamed (the server refuses), so this one is only dropped
        apply(StructureChange::AddColumn {
            name: "score".into(),
            data_type: "int".into(),
            nullable: true,
            default: None,
            comment: None,
            unique: false,
            check: Some("score >= 0".into()),
            generation: crate::db::model::ColumnGeneration::None,
            generated: None,
        })
        .await;
        assert!(column("score").await.is_some());
        let nick = column("nick").await.expect("the column was added");
        assert_eq!((nick.nullable, nick.default.as_deref(), nick.comment.as_str()), (false, Some("x"), "it's"));

        apply(StructureChange::RenameColumn { name: "nick".into(), new_name: "alias".into() }).await;
        assert!(column("nick").await.is_none() && column("alias").await.is_some());
        apply(StructureChange::SetNullable { name: "alias".into(), nullable: true }).await;
        apply(StructureChange::SetDefault { name: "alias".into(), default: Some("'y'".into()) }).await;
        apply(StructureChange::SetComment { name: "alias".into(), comment: Some("hello".into()) }).await;
        apply(StructureChange::SetType { name: "alias".into(), data_type: "varchar(60)".into(), using: None }).await;
        let alias = column("alias").await.unwrap();
        assert_eq!(
            (alias.nullable, alias.default.as_deref(), alias.comment.as_str(), alias.column_type.as_str(), alias.charset.as_deref()),
            (true, Some("y"), "hello", "varchar(60)", Some("utf8mb4"))
        );
        // A number takes no character set
        apply(StructureChange::SetType { name: "age".into(), data_type: "bigint".into(), using: None }).await;
        assert_eq!(column("age").await.unwrap().column_type, "bigint");

        let index = |name: Option<&str>, columns: &[&str], unique: bool, method: &str| StructureChange::CreateIndex {
            name: name.map(String::from),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            unique,
            method: method.into(),
            concurrently: false,
        };
        apply(index(None, &["age"], false, "")).await;
        assert!(exists(&db, "statistics", "index_name", "customers", "customers_age_idx").await);
        apply(index(Some("by_name_hash"), &["name"], false, "hash")).await;
        apply(index(Some("name_ft"), &["name"], false, "fulltext")).await;
        apply(index(Some("lower_email"), &["lower(`email`)"], false, "btree")).await;
        assert!(exists(&db, "statistics", "index_name", "customers", "lower_email").await);
        apply(StructureChange::DropIndex { name: "by_name_hash".into(), cascade: false }).await;
        assert!(!exists(&db, "statistics", "index_name", "customers", "by_name_hash").await);

        apply(StructureChange::AddUnique { name: Some("uq_age_name".into()), columns: vec!["age".into(), "name".into()] }).await;
        apply(StructureChange::AddCheck { name: Some("ck_age".into()), expression: "age < 200".into() }).await;
        assert!(exists(&db, "table_constraints", "constraint_name", "customers", "ck_age").await);
        editor
            .create_table(
                schema,
                "refs",
                NewTable {
                    columns: vec![
                        NewColumn { name: "id".into(), data_type: "int".into(), nullable: false, primary_key: true, default: None, unique: false, check: None, comment: None, generation: crate::db::model::ColumnGeneration::None, generated: None },
                        NewColumn { name: "customer_id".into(), data_type: "int".into(), nullable: true, primary_key: false, default: None, unique: false, check: None, comment: None, generation: crate::db::model::ColumnGeneration::None, generated: None },
                    ],
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        editor
            .apply_change(
                schema,
                "refs",
                StructureChange::AddForeignKey {
                    name: Some("refs_fk".into()),
                    columns: vec!["customer_id".into()],
                    ref_schema: schema.into(),
                    ref_table: "customers".into(),
                    ref_columns: vec!["id".into()],
                    on_update: "".into(),
                    on_delete: "set null".into(),
                },
            )
            .await
            .unwrap();
        assert!(exists(&db, "table_constraints", "constraint_name", "refs", "refs_fk").await);
        editor.apply_change(schema, "refs", StructureChange::DropConstraint { name: "refs_fk".into(), cascade: false }).await.unwrap();
        assert!(!exists(&db, "table_constraints", "constraint_name", "refs", "refs_fk").await);
        apply(StructureChange::DropConstraint { name: "ck_age".into(), cascade: false }).await;
        apply(StructureChange::DropConstraint { name: "uq_age_name".into(), cascade: false }).await;
        assert!(!exists(&db, "table_constraints", "constraint_name", "customers", "ck_age").await);
        assert!(!exists(&db, "table_constraints", "constraint_name", "customers", "uq_age_name").await);

        apply(StructureChange::DropColumn { name: "alias".into(), cascade: false }).await;
        apply(StructureChange::DropColumn { name: "score".into(), cascade: false }).await;
        assert!(column("alias").await.is_none() && column("score").await.is_none());
        let missing = editor.apply_change(schema, "customers", StructureChange::DropConstraint { name: "nope".into(), cascade: false }).await;
        assert_eq!(missing.err().as_deref(), Some("Constraint nope doesn't exist"));

        // Permissions
        let user = format!("dbunny_t_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let _account = Account(format!("'{user}'@'%'"));
        exec(&crate::db::mysql_common::tests_support::test_endpoint(), &format!("CREATE USER '{user}'@'%'")).await;
        apply(StructureChange::Grant { grantee: user.clone(), privileges: vec!["select".into(), "update".into()], grant_option: true }).await;
        let granted = |privilege: &str| {
            let sql = format!(
                "SELECT COUNT(*) FROM information_schema.table_privileges WHERE grantee = \"'{user}'@'%'\" AND table_schema = '{}' AND table_name = 'customers' AND privilege_type = '{privilege}'",
                db.name
            );
            let endpoint = db.endpoint.clone();
            async move { query_one::<i64>(&endpoint, &sql).await.unwrap() > 0 }
        };
        assert!(granted("SELECT").await && granted("UPDATE").await);
        apply(StructureChange::Revoke { grantee: user.clone(), privileges: vec!["update".into()] }).await;
        assert!(granted("SELECT").await && !granted("UPDATE").await);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn create_table_with_keys_and_auto_increment() {
        let (db, d) = open(SHOP).await;
        let editor = d.schema_editor().unwrap();
        let none = crate::db::model::ColumnGeneration::None;
        let spec = NewTable {
            columns: vec![
                NewColumn { name: "id".into(), data_type: "bigint".into(), nullable: false, primary_key: true, default: None, unique: false, check: None, comment: None, generation: crate::db::model::ColumnGeneration::Autoincrement, generated: None },
                NewColumn { name: "email".into(), data_type: "varchar(120)".into(), nullable: false, primary_key: false, default: None, unique: true, check: Some("char_length(`email`) > 0".into()), comment: Some("it's".into()), generation: none, generated: None },
                NewColumn { name: "owner".into(), data_type: "int".into(), nullable: true, primary_key: false, default: None, unique: false, check: None, comment: None, generation: none, generated: None },
            ],
            foreign_keys: vec![NewForeignKey {
                columns: vec!["owner".into()],
                ref_schema: db.name.clone(),
                ref_table: "customers".into(),
                ref_columns: vec!["id".into()],
                on_update: "".into(),
                on_delete: "cascade".into(),
            }],
            ..Default::default()
        };
        let preview = editor.preview_create_table(&db.name, "items", &spec).await.unwrap();
        assert!(preview.starts_with(&format!("CREATE TABLE `{}`.`items` (", db.name)) && preview.ends_with("\n);"), "{preview}");
        editor.create_table(&db.name, "items", spec).await.unwrap();

        let console = d.console().unwrap();
        console.execute("INSERT INTO items (email) VALUES ('a@x.com'), ('b@x.com')").await.unwrap();
        let ids = console.execute("SELECT id FROM items ORDER BY id").await.unwrap();
        assert_eq!(ids.rows, vec![vec![serde_json::json!(1)], vec![serde_json::json!(2)]]);
        assert!(exists(&db, "table_constraints", "constraint_type", "items", "FOREIGN KEY").await);
        assert!(exists(&db, "table_constraints", "constraint_type", "items", "PRIMARY KEY").await);
        let dup = console.execute("INSERT INTO items (email) VALUES ('a@x.com')").await;
        assert!(dup.is_err(), "the email is unique");
        // A refused option reaches the user as it is
        let err = editor.create_table(&db.name, "bad", NewTable { unlogged: true, ..Default::default() }).await.err().unwrap();
        assert_eq!(err, "Unlogged tables isn't supported by MySQL");
    }
}
