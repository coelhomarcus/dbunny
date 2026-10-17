//! Structure changes from the Structure tab: columns, indexes, constraints, triggers and
//! permissions. Names are quoted;
//! types, defaults and index expressions are SQL the user writes, so they're checked
//! to be a single fragment (no `;`) and the generated SQL is shown before it runs.

use async_trait::async_trait;

use super::data::db_message;
use super::objects::{drop_sql, duplicate, relation, rename_sql, run, truncate_sql};
use super::PostgresDriver;
use crate::db::sql::filter::quote_ident;
use crate::db::model::{ColumnGeneration, NewColumn, NewTable, StructureChange};
use crate::db::{unsupported, Engine};
use crate::db::SchemaEditor;

/// Table privileges that can be granted (MAINTAIN only exists from PostgreSQL 17 on).
const TABLE_PRIVILEGES: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER", "MAINTAIN"];

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

/// A role name, quoted, or the PUBLIC keyword (no role can be called "public").
fn grantee(value: &str) -> Result<String, String> {
    if value.trim().eq_ignore_ascii_case("public") {
        Ok("PUBLIC".into())
    } else {
        name("role", value)
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

fn column_list(label: &str, columns: &[String]) -> Result<String, String> {
    if columns.is_empty() {
        return Err(format!("Choose at least one {label}"));
    }
    Ok(columns.iter().map(|c| name(label, c)).collect::<Result<Vec<_>, _>>()?.join(", "))
}

/// `CONSTRAINT "name" ` when one was given, nothing to let the server pick it.
fn constraint_name(value: &Option<String>) -> Result<String, String> {
    match value.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => Ok(format!("CONSTRAINT {} ", name("constraint name", n)?)),
        None => Ok(String::new()),
    }
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
    Ok(quote_ident(value.trim()))
}

fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// An index column: a plain column name is quoted, anything else is an expression.
fn index_column(column: &str) -> Result<String, String> {
    let c = column.trim();
    let is_name = !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
    if is_name {
        Ok(quote_ident(c))
    } else {
        Ok(format!("({})", fragment("index expression", c)?))
    }
}

/// The statements a change runs, and whether they must run outside a transaction.
fn change_sql(schema: &str, table: &str, change: &StructureChange) -> Result<(Vec<String>, bool), String> {
    let target = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let alter = format!("ALTER TABLE {target}");
    let stmts = match change {
        StructureChange::AddColumn { name: column, data_type, nullable, default, comment, unique, check, generation, generated } => {
            let column = name("column name", column)?;
            let spec = NewColumn {
                name: column.clone(),
                data_type: data_type.clone(),
                nullable: *nullable,
                primary_key: false,
                default: default.clone(),
                unique: *unique,
                check: check.clone(),
                comment: None,
                generation: *generation,
                generated: generated.clone(),
            };
            let mut add = format!("{alter} ADD COLUMN {column} {}", fragment("type", data_type)?);
            append_column_tail(&mut add, &spec, false)?;
            let mut stmts = vec![add];
            if let Some(comment) = comment.as_deref().filter(|c| !c.trim().is_empty()) {
                stmts.push(format!("COMMENT ON COLUMN {target}.{column} IS {}", literal(comment)));
            }
            stmts
        }
        StructureChange::RenameColumn { name: column, new_name } => {
            vec![format!("{alter} RENAME COLUMN {} TO {}", name("column name", column)?, name("new name", new_name)?)]
        }
        StructureChange::DropColumn { name: column, cascade } => {
            vec![format!("{alter} DROP COLUMN {}{}", name("column name", column)?, if *cascade { " CASCADE" } else { "" })]
        }
        StructureChange::SetNullable { name: column, nullable } => {
            let action = if *nullable { "DROP NOT NULL" } else { "SET NOT NULL" };
            vec![format!("{alter} ALTER COLUMN {} {action}", name("column name", column)?)]
        }
        StructureChange::SetDefault { name: column, default } => {
            let column = name("column name", column)?;
            match default.as_deref().filter(|d| !d.trim().is_empty()) {
                Some(default) => vec![format!("{alter} ALTER COLUMN {column} SET DEFAULT {}", fragment("default", default)?)],
                None => vec![format!("{alter} ALTER COLUMN {column} DROP DEFAULT")],
            }
        }
        StructureChange::SetComment { name: column, comment } => {
            let value = comment.as_deref().filter(|c| !c.trim().is_empty()).map(literal).unwrap_or_else(|| "NULL".into());
            vec![format!("COMMENT ON COLUMN {target}.{} IS {value}", name("column name", column)?)]
        }
        StructureChange::CreateIndex { name: index, columns, unique, method, concurrently } => {
            if columns.is_empty() {
                return Err("Choose at least one column".into());
            }
            let method = match method.as_str() {
                "btree" | "hash" | "gin" | "gist" | "brin" | "spgist" => method.as_str(),
                other => return Err(format!("Unknown index method \"{other}\"")),
            };
            let cols = columns.iter().map(|c| index_column(c)).collect::<Result<Vec<_>, _>>()?;
            let mut parts = vec![if *unique { "CREATE UNIQUE INDEX" } else { "CREATE INDEX" }.to_string()];
            if *concurrently {
                parts.push("CONCURRENTLY".into());
            }
            if let Some(n) = index.as_deref().filter(|n| !n.trim().is_empty()) {
                parts.push(name("index name", n)?);
            }
            parts.push(format!("ON {target} USING {method} ({})", cols.join(", ")));
            let stmt = parts.join(" ");
            return Ok((vec![stmt], *concurrently));
        }
        StructureChange::DropIndex { name: index, cascade } => {
            vec![format!(
                "DROP INDEX {}.{}{}",
                quote_ident(schema),
                name("index name", index)?,
                if *cascade { " CASCADE" } else { "" }
            )]
        }
        StructureChange::SetType { name: column, data_type, using } => {
            let column = name("column name", column)?;
            let mut stmt = format!("{alter} ALTER COLUMN {column} TYPE {}", fragment("type", data_type)?);
            if let Some(using) = using.as_deref().filter(|u| !u.trim().is_empty()) {
                stmt += &format!(" USING {}", fragment("conversion", using)?);
            }
            vec![stmt]
        }
        StructureChange::AddForeignKey { name: constraint, columns, ref_schema, ref_table, ref_columns, on_update, on_delete } => {
            if columns.len() != ref_columns.len() {
                return Err("Pick as many referenced columns as columns".into());
            }
            vec![format!(
                "{alter} ADD {}FOREIGN KEY ({}) REFERENCES {}.{} ({}) ON UPDATE {} ON DELETE {}",
                constraint_name(constraint)?,
                column_list("column", columns)?,
                name("referenced schema", ref_schema)?,
                name("referenced table", ref_table)?,
                column_list("referenced column", ref_columns)?,
                referential_action("update", on_update)?,
                referential_action("delete", on_delete)?,
            )]
        }
        StructureChange::AddUnique { name: constraint, columns } => {
            vec![format!("{alter} ADD {}UNIQUE ({})", constraint_name(constraint)?, column_list("column", columns)?)]
        }
        StructureChange::AddCheck { name: constraint, expression } => {
            vec![format!("{alter} ADD {}CHECK ({})", constraint_name(constraint)?, fragment("check", expression)?)]
        }
        StructureChange::DropConstraint { name: constraint, cascade } => {
            vec![format!("{alter} DROP CONSTRAINT {}{}", name("constraint name", constraint)?, if *cascade { " CASCADE" } else { "" })]
        }
        StructureChange::SetTriggerEnabled { name: trigger, enabled } => {
            let action = if *enabled { "ENABLE" } else { "DISABLE" };
            vec![format!("{alter} {action} TRIGGER {}", name("trigger name", trigger)?)]
        }
        StructureChange::Grant { grantee: role, privileges: list, grant_option } => {
            let mut stmt = format!("GRANT {} ON TABLE {target} TO {}", privileges(list)?, grantee(role)?);
            if *grant_option {
                stmt += " WITH GRANT OPTION";
            }
            vec![stmt]
        }
        StructureChange::Revoke { grantee: role, privileges: list } => {
            vec![format!("REVOKE {} ON TABLE {target} FROM {}", privileges(list)?, grantee(role)?)]
        }
    };
    Ok((stmts, false))
}

async fn apply(client: &tokio_postgres::Client, schema: &str, table: &str, change: &StructureChange) -> Result<(), String> {
    let (stmts, outside_transaction) = change_sql(schema, table, change)?;
    if outside_transaction {
        // CREATE INDEX CONCURRENTLY refuses to run inside a transaction
        return client.batch_execute(&stmts.join(";\n")).await.map_err(|e| db_message(&e));
    }
    // One message: nothing else of the session runs in the middle of the transaction
    let sql = format!("BEGIN; {}; COMMIT;", stmts.join("; "));
    if let Err(e) = client.batch_execute(&sql).await {
        let _ = client.batch_execute("ROLLBACK").await;
        return Err(db_message(&e));
    }
    Ok(())
}

/// `CREATE TABLE` from the table builder. Several primary key columns make a composite key.
fn create_table_sql(schema: &str, table: &str, spec: &NewTable) -> Result<String, String> {
    let columns = &spec.columns;
    if spec.without_rowid || spec.strict {
        return Err(unsupported(Engine::Postgres, "WITHOUT ROWID and STRICT"));
    }
    let target = format!("{}.{}", quote_ident(schema), name("table name", table)?);
    if columns.is_empty() {
        return Err("Add at least one column".into());
    }
    let mut seen = Vec::new();
    let mut lines = Vec::new();
    for column in columns {
        let col = name("column name", &column.name)?;
        if seen.contains(&col) {
            return Err(format!("Column {} appears twice", column.name.trim()));
        }
        seen.push(col.clone());
        let mut line = format!("{col} {}", fragment("type", &column.data_type)?);
        append_column_tail(&mut line, column, true)?;
        lines.push(line);
    }
    let keys: Vec<String> = columns.iter().filter(|c| c.primary_key).map(|c| quote_ident(c.name.trim())).collect();
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
    let logged = if spec.unlogged { "UNLOGGED " } else { "" };
    let mut sql = format!("CREATE {logged}TABLE {target} (\n    {}\n)", lines.join(",\n    "));
    if let Some(partition) = &spec.partition {
        let method = match partition.method.trim().to_uppercase().as_str() {
            "RANGE" => "RANGE",
            "LIST" => "LIST",
            "HASH" => "HASH",
            other => return Err(format!("Unknown partition method \"{other}\"")),
        };
        sql += &format!(" PARTITION BY {method} ({})", fragment("partition key", &partition.expression)?);
    }
    for column in columns {
        let Some(comment) = column.comment.as_deref().filter(|c| !c.trim().is_empty()) else { continue };
        let col = name("column name", &column.name)?;
        sql += &format!(";\nCOMMENT ON COLUMN {target}.{col} IS {}", literal(comment));
    }
    Ok(sql)
}

/// Default, identity or `GENERATED`, then `NOT NULL`, `UNIQUE` and `CHECK`.
/// A primary key of a new table is already `NOT NULL`, so that word is left off there.
fn append_column_tail(line: &mut String, column: &NewColumn, skip_not_null_on_primary_key: bool) -> Result<(), String> {
    let has_default = column.default.as_deref().is_some_and(|d| !d.trim().is_empty());
    match column.generation {
        ColumnGeneration::None => {
            if has_default {
                *line += &format!(" DEFAULT {}", fragment("default", column.default.as_deref().unwrap_or(""))?);
            }
        }
        ColumnGeneration::IdentityByDefault | ColumnGeneration::IdentityAlways => {
            if has_default {
                return Err("An identity column can't have a default".into());
            }
            let kind = if column.generation == ColumnGeneration::IdentityAlways { "ALWAYS" } else { "BY DEFAULT" };
            *line += &format!(" GENERATED {kind} AS IDENTITY");
        }
        ColumnGeneration::Autoincrement => return Err(unsupported(Engine::Postgres, "AUTOINCREMENT")),
        ColumnGeneration::Generated => {
            if has_default {
                return Err("A generated column can't have a default".into());
            }
            let expr = column.generated.as_deref().filter(|e| !e.trim().is_empty()).ok_or("The generated expression can't be empty")?;
            *line += &format!(" GENERATED ALWAYS AS ({}) STORED", fragment("generated expression", expr)?);
        }
    }
    if !column.nullable && !(skip_not_null_on_primary_key && column.primary_key) {
        *line += " NOT NULL";
    }
    column_flags(line, column.unique, &column.check)
}

/// `UNIQUE` and `CHECK (...)` on a column definition. An empty check is left off.
fn column_flags(line: &mut String, unique: bool, check: &Option<String>) -> Result<(), String> {
    if unique {
        *line += " UNIQUE";
    }
    if let Some(check) = check.as_deref().filter(|c| !c.trim().is_empty()) {
        *line += &format!(" CHECK ({})", fragment("check", check)?);
    }
    Ok(())
}

#[async_trait]
impl SchemaEditor for PostgresDriver {
    /// The SQL a change would run, for showing before running it.
    async fn preview_change(&self, schema: &str, table: &str, change: &StructureChange) -> Result<String, String> {
        let (stmts, _) = change_sql(schema, table, change)?;
        Ok(stmts.iter().map(|s| format!("{s};")).collect::<Vec<_>>().join("\n"))
    }

    async fn apply_change(&self, schema: &str, table: &str, change: StructureChange) -> Result<(), String> {
        let client = self.client().await?;
        apply(&client, schema, table, &change).await
    }

    async fn preview_create_table(&self, schema: &str, table: &str, spec: &NewTable) -> Result<String, String> {
        Ok(create_table_sql(schema, table, spec)? + ";")
    }

    async fn create_table(&self, schema: &str, table: &str, spec: NewTable) -> Result<(), String> {
        let sql = create_table_sql(schema, table, &spec)?;
        let client = self.client().await?;
        client.batch_execute(&sql).await.map_err(|e| db_message(&e))
    }

    async fn rename_relation(&self, schema: &str, name: &str, kind: &str, new_name: &str) -> Result<(), String> {
        run(self, &rename_sql(schema, name, kind, new_name)?).await
    }

    async fn drop_relation(&self, schema: &str, name: &str, kind: &str, cascade: bool) -> Result<(), String> {
        run(self, &drop_sql(schema, name, kind, cascade)?).await
    }

    async fn truncate_table(&self, schema: &str, name: &str, restart_identity: bool, cascade: bool) -> Result<(), String> {
        run(self, &truncate_sql(schema, name, restart_identity, cascade)).await
    }

    async fn duplicate_table(&self, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String> {
        let client = self.client().await?;
        duplicate(&client, schema, name, new_name, with_data).await
    }

    async fn refresh_materialized_view(&self, schema: &str, name: &str) -> Result<(), String> {
        run(self, &format!("REFRESH MATERIALIZED VIEW {}", relation(schema, name))).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::model::{NewForeignKey, NewPartition};

    fn sql(change: StructureChange) -> String {
        change_sql("public", "users", &change).map(|(s, _)| s.join("; ")).unwrap_or_else(|e| format!("ERR {e}"))
    }

    #[test]
    fn column_changes_quote_names() {
        assert_eq!(
            sql(StructureChange::AddColumn { name: "full name".into(), data_type: "varchar(80)".into(), nullable: false, default: Some("'x'".into()), comment: Some("it's".into()), unique: false, check: None, generation: ColumnGeneration::None, generated: None }),
            "ALTER TABLE \"public\".\"users\" ADD COLUMN \"full name\" varchar(80) DEFAULT 'x' NOT NULL; COMMENT ON COLUMN \"public\".\"users\".\"full name\" IS 'it''s'"
        );
        assert_eq!(
            sql(StructureChange::RenameColumn { name: "a\"b".into(), new_name: "c".into() }),
            "ALTER TABLE \"public\".\"users\" RENAME COLUMN \"a\"\"b\" TO \"c\""
        );
        assert_eq!(sql(StructureChange::SetNullable { name: "x".into(), nullable: true }), "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"x\" DROP NOT NULL");
        assert_eq!(sql(StructureChange::SetDefault { name: "x".into(), default: Some(" ".into()) }), "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"x\" DROP DEFAULT");
        assert_eq!(sql(StructureChange::SetComment { name: "x".into(), comment: None }), "COMMENT ON COLUMN \"public\".\"users\".\"x\" IS NULL");
        assert_eq!(
            sql(StructureChange::AddColumn { name: "amount".into(), data_type: "numeric(12,2)".into(), nullable: false, default: Some("0".into()), comment: None, unique: true, check: Some("amount > 0".into()), generation: ColumnGeneration::None, generated: None }),
            "ALTER TABLE \"public\".\"users\" ADD COLUMN \"amount\" numeric(12,2) DEFAULT 0 NOT NULL UNIQUE CHECK (amount > 0)"
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
                generation: ColumnGeneration::IdentityAlways,
                generated: None,
            }),
            "ALTER TABLE \"public\".\"users\" ADD COLUMN \"id\" bigint GENERATED ALWAYS AS IDENTITY NOT NULL"
        );
    }

    #[test]
    fn fragments_with_semicolons_and_empty_names_are_refused() {
        assert!(sql(StructureChange::AddColumn { name: "x".into(), data_type: "int; DROP TABLE users".into(), nullable: true, default: None, comment: None, unique: false, check: None, generation: ColumnGeneration::None, generated: None }).starts_with("ERR"));
        assert!(sql(StructureChange::AddColumn { name: "x".into(), data_type: "int".into(), nullable: true, default: None, comment: None, unique: false, check: Some("x > 0; --".into()), generation: ColumnGeneration::None, generated: None }).starts_with("ERR"));
        assert!(sql(StructureChange::SetDefault { name: "x".into(), default: Some("1; --".into()) }).starts_with("ERR"));
        assert!(sql(StructureChange::RenameColumn { name: "x".into(), new_name: " ".into() }).starts_with("ERR"));
    }

    fn spec(columns: Vec<NewColumn>) -> NewTable {
        NewTable { columns, ..NewTable::default() }
    }

    fn new_column(name: &str, data_type: &str, nullable: bool, primary_key: bool) -> NewColumn {
        NewColumn {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            primary_key,
            default: None,
            unique: false,
            check: None,
            comment: None,
            generation: ColumnGeneration::None,
            generated: None,
        }
    }

    #[test]
    fn create_table_builds_columns_and_a_composite_key() {
        let sql = create_table_sql(
            "public",
            "order items",
            &spec(vec![
                new_column("order_id", "bigint", false, true),
                new_column("line", "int", false, true),
                NewColumn { default: Some("now()".into()), ..new_column("added", "timestamptz", false, false) },
                new_column("note", "text", true, false),
            ]),
        )
        .unwrap();
        assert_eq!(
            sql,
            "CREATE TABLE \"public\".\"order items\" (\n    \"order_id\" bigint,\n    \"line\" int,\n    \"added\" timestamptz DEFAULT now() NOT NULL,\n    \"note\" text,\n    PRIMARY KEY (\"order_id\", \"line\")\n)"
        );
        assert!(create_table_sql("public", "t", &spec(vec![])).is_err());
        assert!(create_table_sql("public", " ", &spec(vec![new_column("a", "int", true, false)])).is_err());
        assert!(create_table_sql("public", "t", &spec(vec![new_column("a", "int", true, false), new_column("a", "text", true, false)])).unwrap_err().contains("twice"));
        assert!(create_table_sql("public", "t", &spec(vec![new_column("a", "int); DROP TABLE x; --", true, false)])).is_err());

        let flagged = create_table_sql(
            "public",
            "prices",
            &spec(vec![NewColumn { unique: true, check: Some("amount > 0".into()), comment: Some("unit price".into()), ..new_column("amount", "numeric(12,2)", false, false) }]),
        )
        .unwrap();
        assert_eq!(
            flagged,
            "CREATE TABLE \"public\".\"prices\" (\n    \"amount\" numeric(12,2) NOT NULL UNIQUE CHECK (amount > 0)\n);\nCOMMENT ON COLUMN \"public\".\"prices\".\"amount\" IS 'unit price'"
        );
        assert!(create_table_sql("public", "t", &spec(vec![NewColumn { check: Some("a > 0; --".into()), ..new_column("a", "int", true, false) }])).is_err());

        let identity = create_table_sql("public", "people", &spec(vec![NewColumn { generation: ColumnGeneration::IdentityByDefault, ..new_column("id", "bigint", false, true) }])).unwrap();
        assert_eq!(identity, "CREATE TABLE \"public\".\"people\" (\n    \"id\" bigint GENERATED BY DEFAULT AS IDENTITY,\n    PRIMARY KEY (\"id\")\n)");
        let always = create_table_sql("public", "people", &spec(vec![NewColumn { generation: ColumnGeneration::IdentityAlways, ..new_column("id", "bigint", false, true) }])).unwrap();
        assert!(always.contains("GENERATED ALWAYS AS IDENTITY"), "{always}");
        let generated = create_table_sql(
            "public",
            "prices",
            &spec(vec![
                new_column("amount", "numeric", false, false),
                NewColumn { generation: ColumnGeneration::Generated, generated: Some("amount * 2".into()), ..new_column("doubled", "numeric", false, false) },
            ]),
        )
        .unwrap();
        assert!(generated.contains("\"doubled\" numeric GENERATED ALWAYS AS (amount * 2) STORED NOT NULL"), "{generated}");
        assert!(create_table_sql("public", "t", &spec(vec![NewColumn { generation: ColumnGeneration::Generated, generated: Some("a; --".into()), ..new_column("a", "int", true, false) }])).is_err());
        assert!(create_table_sql("public", "t", &spec(vec![NewColumn { generation: ColumnGeneration::IdentityByDefault, default: Some("1".into()), ..new_column("id", "int", false, true) }])).is_err());
    }

    #[test]
    fn create_table_writes_a_foreign_key_and_table_options() {
        let table = NewTable {
            columns: vec![new_column("user_id", "bigint", true, false), new_column("created_at", "timestamptz", false, false)],
            foreign_keys: vec![NewForeignKey {
                columns: vec!["user_id".into()],
                ref_schema: "public".into(),
                ref_table: "users".into(),
                ref_columns: vec!["id".into()],
                on_update: "cascade".into(),
                on_delete: "set null".into(),
            }],
            unlogged: true,
            partition: Some(NewPartition { method: "range".into(), expression: "created_at".into() }),
            ..NewTable::default()
        };
        assert_eq!(
            create_table_sql("public", "events", &table).unwrap(),
            "CREATE UNLOGGED TABLE \"public\".\"events\" (\n    \"user_id\" bigint,\n    \"created_at\" timestamptz NOT NULL,\n    FOREIGN KEY (\"user_id\") REFERENCES \"public\".\"users\" (\"id\") ON UPDATE CASCADE ON DELETE SET NULL\n) PARTITION BY RANGE (created_at)"
        );
        assert!(create_table_sql("public", "t", &NewTable { without_rowid: true, ..spec(vec![new_column("a", "int", false, true)]) }).unwrap_err().contains("WITHOUT ROWID"));
        assert!(create_table_sql("public", "t", &NewTable { partition: Some(NewPartition { method: "tree".into(), expression: "a".into() }), ..spec(vec![new_column("a", "int", true, false)]) }).is_err());
        assert!(create_table_sql("public", "t", &NewTable { partition: Some(NewPartition { method: "list".into(), expression: "a; --".into() }), ..spec(vec![new_column("a", "int", true, false)]) }).is_err());
    }

    #[test]
    fn indexes_quote_columns_and_wrap_expressions() {
        let (stmts, outside) = change_sql(
            "public",
            "users",
            &StructureChange::CreateIndex { name: Some("users_lower_email".into()), columns: vec!["lower(email)".into(), "created_at".into()], unique: true, method: "btree".into(), concurrently: true },
        )
        .unwrap();
        assert_eq!(stmts[0], "CREATE UNIQUE INDEX CONCURRENTLY \"users_lower_email\" ON \"public\".\"users\" USING btree ((lower(email)), \"created_at\")");
        assert!(outside, "concurrently runs outside a transaction");
        assert_eq!(
            sql(StructureChange::CreateIndex { name: None, columns: vec!["email".into()], unique: false, method: "hash".into(), concurrently: false }),
            "CREATE INDEX ON \"public\".\"users\" USING hash (\"email\")"
        );
        assert!(sql(StructureChange::CreateIndex { name: None, columns: vec!["email".into()], unique: false, method: "evil".into(), concurrently: false }).starts_with("ERR"));
        assert!(sql(StructureChange::CreateIndex { name: None, columns: vec![], unique: false, method: "btree".into(), concurrently: false }).starts_with("ERR"));
        assert_eq!(sql(StructureChange::DropIndex { name: "i".into(), cascade: false }), "DROP INDEX \"public\".\"i\"");
    }

    #[test]
    fn constraints_and_types_build_their_sql() {
        assert_eq!(
            sql(StructureChange::SetType { name: "price".into(), data_type: "numeric(10,2)".into(), using: Some("price::numeric".into()) }),
            "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"price\" TYPE numeric(10,2) USING price::numeric"
        );
        assert!(sql(StructureChange::SetType { name: "x".into(), data_type: "int".into(), using: Some("1; DROP TABLE t".into()) }).starts_with("ERR"));
        assert_eq!(
            sql(StructureChange::AddForeignKey {
                name: Some("users_team_fk".into()),
                columns: vec!["team_id".into()],
                ref_schema: "public".into(),
                ref_table: "teams".into(),
                ref_columns: vec!["id".into()],
                on_update: "".into(),
                on_delete: "set null".into(),
            }),
            "ALTER TABLE \"public\".\"users\" ADD CONSTRAINT \"users_team_fk\" FOREIGN KEY (\"team_id\") REFERENCES \"public\".\"teams\" (\"id\") ON UPDATE NO ACTION ON DELETE SET NULL"
        );
        let fk = |on_delete: &str, ref_columns: Vec<String>| StructureChange::AddForeignKey {
            name: None,
            columns: vec!["a".into()],
            ref_schema: "public".into(),
            ref_table: "t".into(),
            ref_columns,
            on_update: "CASCADE".into(),
            on_delete: on_delete.into(),
        };
        assert!(sql(fk("DROP TABLE", vec!["id".into()])).starts_with("ERR"));
        assert!(sql(fk("CASCADE", vec!["id".into(), "x".into()])).contains("as many"));
        assert_eq!(sql(StructureChange::AddUnique { name: None, columns: vec!["email".into(), "org".into()] }), "ALTER TABLE \"public\".\"users\" ADD UNIQUE (\"email\", \"org\")");
        assert_eq!(
            sql(StructureChange::AddCheck { name: Some("age_ok".into()), expression: "age >= 0".into() }),
            "ALTER TABLE \"public\".\"users\" ADD CONSTRAINT \"age_ok\" CHECK (age >= 0)"
        );
        assert_eq!(sql(StructureChange::DropConstraint { name: "age_ok".into(), cascade: true }), "ALTER TABLE \"public\".\"users\" DROP CONSTRAINT \"age_ok\" CASCADE");
        assert_eq!(sql(StructureChange::SetTriggerEnabled { name: "audit".into(), enabled: false }), "ALTER TABLE \"public\".\"users\" DISABLE TRIGGER \"audit\"");
    }

    #[test]
    fn grants_check_privileges_and_quote_roles() {
        assert_eq!(
            sql(StructureChange::Grant { grantee: "app user".into(), privileges: vec!["select".into(), "UPDATE".into()], grant_option: true }),
            "GRANT SELECT, UPDATE ON TABLE \"public\".\"users\" TO \"app user\" WITH GRANT OPTION"
        );
        assert_eq!(
            sql(StructureChange::Revoke { grantee: "public".into(), privileges: vec!["DELETE".into()] }),
            "REVOKE DELETE ON TABLE \"public\".\"users\" FROM PUBLIC"
        );
        assert!(sql(StructureChange::Grant { grantee: "x".into(), privileges: vec!["ALL; DROP TABLE t".into()], grant_option: false }).starts_with("ERR"));
        assert!(sql(StructureChange::Revoke { grantee: "x".into(), privileges: vec![] }).starts_with("ERR"));
    }
}

/// Tests that need a real server, see `db::postgres::console::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;

    async fn connect() -> tokio_postgres::Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn every_change_runs_on_a_real_server() {
        let client = connect().await;
        let s = "dbunny_alter_test";
        client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}; CREATE TABLE {s}.t (id int PRIMARY KEY, email text); INSERT INTO {s}.t VALUES (1, 'A@x.io')"))
            .await
            .unwrap();
        let run = |change: StructureChange| {
            let client = &client;
            async move { apply(client, s, "t", &change).await }
        };

        run(StructureChange::AddColumn { name: "score".into(), data_type: "numeric(5,2)".into(), nullable: false, default: Some("0".into()), comment: Some("Points".into()), unique: false, check: None, generation: ColumnGeneration::None, generated: None }).await.unwrap();
        run(StructureChange::RenameColumn { name: "score".into(), new_name: "points".into() }).await.unwrap();
        run(StructureChange::SetDefault { name: "points".into(), default: Some("10".into()) }).await.unwrap();
        run(StructureChange::SetNullable { name: "points".into(), nullable: true }).await.unwrap();
        run(StructureChange::SetComment { name: "points".into(), comment: Some("Total points".into()) }).await.unwrap();
        run(StructureChange::CreateIndex { name: Some("t_email_lower".into()), columns: vec!["lower(email)".into()], unique: true, method: "btree".into(), concurrently: false }).await.unwrap();
        run(StructureChange::CreateIndex { name: Some("t_points".into()), columns: vec!["points".into()], unique: false, method: "btree".into(), concurrently: true }).await.unwrap();

        let row = client
            .query_one(
                &format!("SELECT column_default, is_nullable, col_description('{s}.t'::regclass, 3) FROM information_schema.columns WHERE table_schema = '{s}' AND table_name = 't' AND column_name = 'points'"),
                &[],
            )
            .await
            .unwrap();
        assert_eq!(row.get::<_, Option<String>>(0).as_deref(), Some("10"));
        assert_eq!(row.get::<_, String>(1), "YES");
        assert_eq!(row.get::<_, Option<String>>(2).as_deref(), Some("Total points"));

        // The unique expression index works, and a failed change leaves no transaction open
        let err = client.batch_execute(&format!("INSERT INTO {s}.t (id, email) VALUES (2, 'a@x.io')")).await.unwrap_err();
        assert!(db_message(&err).contains("duplicate key"));
        let err = run(StructureChange::SetNullable { name: "id".into(), nullable: true }).await.unwrap_err();
        assert!(err.contains("primary key"), "{err}");
        client.query_one("SELECT 1", &[]).await.unwrap();

        // Constraints, a type change, triggers and permissions
        client
            .batch_execute(&format!(
                "CREATE TABLE {s}.teams (id int PRIMARY KEY); INSERT INTO {s}.teams VALUES (7); \
                 ALTER TABLE {s}.t ADD COLUMN team_id text, ADD COLUMN age int; UPDATE {s}.t SET team_id = '7'; \
                 CREATE FUNCTION {s}.noop() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$; \
                 CREATE TRIGGER t_noop BEFORE INSERT ON {s}.t FOR EACH ROW EXECUTE FUNCTION {s}.noop(); \
                 DROP ROLE IF EXISTS dbunny_grant_test; CREATE ROLE dbunny_grant_test"
            ))
            .await
            .unwrap();
        run(StructureChange::SetType { name: "team_id".into(), data_type: "int".into(), using: Some("team_id::int".into()) }).await.unwrap();
        run(StructureChange::AddForeignKey {
            name: Some("t_team_fk".into()),
            columns: vec!["team_id".into()],
            ref_schema: s.into(),
            ref_table: "teams".into(),
            ref_columns: vec!["id".into()],
            on_update: "CASCADE".into(),
            on_delete: "SET NULL".into(),
        })
        .await
        .unwrap();
        run(StructureChange::AddCheck { name: Some("t_age_ok".into()), expression: "age >= 0".into() }).await.unwrap();
        run(StructureChange::AddUnique { name: None, columns: vec!["team_id".into(), "age".into()] }).await.unwrap();
        let err = client.batch_execute(&format!("INSERT INTO {s}.t (id, age) VALUES (5, -1)")).await.unwrap_err();
        assert!(db_message(&err).contains("t_age_ok"), "{}", db_message(&err));
        run(StructureChange::DropConstraint { name: "t_age_ok".into(), cascade: false }).await.unwrap();
        run(StructureChange::DropConstraint { name: "t_team_fk".into(), cascade: false }).await.unwrap();
        let kinds: Vec<String> = client
            .query(&format!("SELECT contype::text FROM pg_constraint WHERE conrelid = '{s}.t'::regclass ORDER BY 1"), &[])
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        assert_eq!(kinds, vec!["p", "u"], "the primary key and the unique constraint are left");

        run(StructureChange::SetTriggerEnabled { name: "t_noop".into(), enabled: false }).await.unwrap();
        let enabled: String = client.query_one("SELECT tgenabled::text FROM pg_trigger WHERE tgname = 't_noop'", &[]).await.unwrap().get(0);
        assert_eq!(enabled, "D");
        run(StructureChange::SetTriggerEnabled { name: "t_noop".into(), enabled: true }).await.unwrap();

        run(StructureChange::Grant { grantee: "dbunny_grant_test".into(), privileges: vec!["SELECT".into(), "INSERT".into()], grant_option: false }).await.unwrap();
        run(StructureChange::Revoke { grantee: "dbunny_grant_test".into(), privileges: vec!["INSERT".into()] }).await.unwrap();
        let rights: Vec<bool> = client
            .query_one(
                &format!("SELECT has_table_privilege('dbunny_grant_test', '{s}.t', 'SELECT'), has_table_privilege('dbunny_grant_test', '{s}.t', 'INSERT')"),
                &[],
            )
            .await
            .map(|r| vec![r.get(0), r.get(1)])
            .unwrap();
        assert_eq!(rights, vec![true, false]);
        run(StructureChange::Revoke { grantee: "dbunny_grant_test".into(), privileges: vec!["SELECT".into()] }).await.unwrap();
        client.batch_execute(&format!("ALTER TABLE {s}.t DROP COLUMN team_id, DROP COLUMN age; DROP TABLE {s}.teams")).await.unwrap();

        run(StructureChange::DropIndex { name: "t_email_lower".into(), cascade: false }).await.unwrap();
        run(StructureChange::DropColumn { name: "points".into(), cascade: false }).await.unwrap();
        let count: i64 = client.query_one(&format!("SELECT count(*) FROM pg_indexes WHERE schemaname = '{s}'"), &[]).await.unwrap().get(0);
        assert_eq!(count, 1, "only the primary key is left");
        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE; DROP ROLE dbunny_grant_test")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn created_tables_work() {
        let client = connect().await;
        let s = "dbunny_create_test";
        client.batch_execute(&format!("DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}")).await.unwrap();
        let sql = create_table_sql(
            s,
            "people",
            &NewTable {
                columns: vec![
                    NewColumn { name: "id".into(), data_type: "bigserial".into(), nullable: false, primary_key: true, default: None, unique: false, check: None, comment: None, generation: ColumnGeneration::None, generated: None },
                    NewColumn { name: "name".into(), data_type: "text".into(), nullable: false, primary_key: false, default: None, unique: false, check: None, comment: None, generation: ColumnGeneration::None, generated: None },
                    NewColumn { name: "joined".into(), data_type: "date".into(), nullable: true, primary_key: false, default: Some("current_date".into()), unique: false, check: None, comment: None, generation: ColumnGeneration::None, generated: None },
                ],
                ..NewTable::default()
            },
        )
        .unwrap();
        client.batch_execute(&sql).await.unwrap();
        client.batch_execute(&format!("INSERT INTO {s}.people (name) VALUES ('ana')")).await.unwrap();
        let row = client.query_one(&format!("SELECT id, joined = current_date FROM {s}.people"), &[]).await.unwrap();
        assert_eq!(row.get::<_, i64>(0), 1);
        assert!(row.get::<_, bool>(1));
        let err = client.batch_execute(&format!("INSERT INTO {s}.people (name) VALUES (NULL)")).await.unwrap_err();
        assert!(db_message(&err).contains("null value"));
        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }
}
