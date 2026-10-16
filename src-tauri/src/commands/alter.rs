//! Structure changes from the Structure tab: columns, indexes, constraints, triggers and
//! permissions. Names are quoted;
//! types, defaults and index expressions are SQL the user writes, so they're checked
//! to be a single fragment (no `;`) and the generated SQL is shown before it runs.

use serde::Deserialize;
use tauri::State;

use super::query::db_message;
use super::state::AppState;
use super::tablefilter::quote_ident;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StructureChange {
    AddColumn { name: String, data_type: String, nullable: bool, default: Option<String>, comment: Option<String> },
    RenameColumn { name: String, new_name: String },
    DropColumn { name: String, cascade: bool },
    SetNullable { name: String, nullable: bool },
    SetDefault { name: String, default: Option<String> },
    SetComment { name: String, comment: Option<String> },
    /// `columns` are column names or expressions such as `lower(email)`.
    CreateIndex { name: Option<String>, columns: Vec<String>, unique: bool, method: String, concurrently: bool },
    DropIndex { name: String, cascade: bool },
    /// `using` converts the existing values when there's no automatic cast, e.g. `price::numeric`.
    SetType { name: String, data_type: String, using: Option<String> },
    AddForeignKey {
        name: Option<String>,
        columns: Vec<String>,
        ref_schema: String,
        ref_table: String,
        ref_columns: Vec<String>,
        on_update: String,
        on_delete: String,
    },
    AddUnique { name: Option<String>, columns: Vec<String> },
    AddCheck { name: Option<String>, expression: String },
    DropConstraint { name: String, cascade: bool },
    SetTriggerEnabled { name: String, enabled: bool },
    /// `grantee` is a role name, or PUBLIC for everyone.
    Grant { grantee: String, privileges: Vec<String>, grant_option: bool },
    Revoke { grantee: String, privileges: Vec<String> },
}

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
pub(crate) fn change_sql(schema: &str, table: &str, change: &StructureChange) -> Result<(Vec<String>, bool), String> {
    let target = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let alter = format!("ALTER TABLE {target}");
    let stmts = match change {
        StructureChange::AddColumn { name: column, data_type, nullable, default, comment } => {
            let column = name("column name", column)?;
            let mut add = format!("{alter} ADD COLUMN {column} {}", fragment("type", data_type)?);
            if let Some(default) = default.as_deref().filter(|d| !d.trim().is_empty()) {
                add += &format!(" DEFAULT {}", fragment("default", default)?);
            }
            if !nullable {
                add += " NOT NULL";
            }
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

/// The SQL a change would run, for showing before running it.
#[tauri::command]
pub fn preview_structure_change(schema: String, table: String, change: StructureChange) -> Result<String, String> {
    let (stmts, _) = change_sql(&schema, &table, &change)?;
    Ok(stmts.iter().map(|s| format!("{s};")).collect::<Vec<_>>().join("\n"))
}

#[tauri::command]
pub async fn apply_structure_change(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    change: StructureChange,
) -> Result<(), String> {
    let client = state.client(&session_id).await?;
    apply(&client, &schema, &table, &change).await
}

pub(crate) async fn apply(client: &tokio_postgres::Client, schema: &str, table: &str, change: &StructureChange) -> Result<(), String> {
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

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub default: Option<String>,
}

/// `CREATE TABLE` from the table builder. Several primary key columns make a composite key.
pub(crate) fn create_table_sql(schema: &str, table: &str, columns: &[NewColumn]) -> Result<String, String> {
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
        if let Some(default) = column.default.as_deref().filter(|d| !d.trim().is_empty()) {
            line += &format!(" DEFAULT {}", fragment("default", default)?);
        }
        // A primary key is NOT NULL anyway
        if !column.nullable && !column.primary_key {
            line += " NOT NULL";
        }
        lines.push(line);
    }
    let keys: Vec<String> = columns.iter().filter(|c| c.primary_key).map(|c| quote_ident(c.name.trim())).collect();
    if !keys.is_empty() {
        lines.push(format!("PRIMARY KEY ({})", keys.join(", ")));
    }
    Ok(format!("CREATE TABLE {target} (\n    {}\n)", lines.join(",\n    ")))
}

#[tauri::command]
pub fn preview_create_table(schema: String, table: String, columns: Vec<NewColumn>) -> Result<String, String> {
    Ok(create_table_sql(&schema, &table, &columns)? + ";")
}

#[tauri::command]
pub async fn create_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
    columns: Vec<NewColumn>,
) -> Result<(), String> {
    let sql = create_table_sql(&schema, &table, &columns)?;
    let client = state.client(&session_id).await?;
    client.batch_execute(&sql).await.map_err(|e| db_message(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sql(change: StructureChange) -> String {
        change_sql("public", "users", &change).map(|(s, _)| s.join("; ")).unwrap_or_else(|e| format!("ERR {e}"))
    }

    #[test]
    fn column_changes_quote_names() {
        assert_eq!(
            sql(StructureChange::AddColumn { name: "full name".into(), data_type: "varchar(80)".into(), nullable: false, default: Some("'x'".into()), comment: Some("it's".into()) }),
            "ALTER TABLE \"public\".\"users\" ADD COLUMN \"full name\" varchar(80) DEFAULT 'x' NOT NULL; COMMENT ON COLUMN \"public\".\"users\".\"full name\" IS 'it''s'"
        );
        assert_eq!(
            sql(StructureChange::RenameColumn { name: "a\"b".into(), new_name: "c".into() }),
            "ALTER TABLE \"public\".\"users\" RENAME COLUMN \"a\"\"b\" TO \"c\""
        );
        assert_eq!(sql(StructureChange::SetNullable { name: "x".into(), nullable: true }), "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"x\" DROP NOT NULL");
        assert_eq!(sql(StructureChange::SetDefault { name: "x".into(), default: Some(" ".into()) }), "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"x\" DROP DEFAULT");
        assert_eq!(sql(StructureChange::SetComment { name: "x".into(), comment: None }), "COMMENT ON COLUMN \"public\".\"users\".\"x\" IS NULL");
    }

    #[test]
    fn fragments_with_semicolons_and_empty_names_are_refused() {
        assert!(sql(StructureChange::AddColumn { name: "x".into(), data_type: "int; DROP TABLE users".into(), nullable: true, default: None, comment: None }).starts_with("ERR"));
        assert!(sql(StructureChange::SetDefault { name: "x".into(), default: Some("1; --".into()) }).starts_with("ERR"));
        assert!(sql(StructureChange::RenameColumn { name: "x".into(), new_name: " ".into() }).starts_with("ERR"));
    }

    fn new_column(name: &str, data_type: &str, nullable: bool, primary_key: bool) -> NewColumn {
        NewColumn { name: name.into(), data_type: data_type.into(), nullable, primary_key, default: None }
    }

    #[test]
    fn create_table_builds_columns_and_a_composite_key() {
        let sql = create_table_sql(
            "public",
            "order items",
            &[
                new_column("order_id", "bigint", false, true),
                new_column("line", "int", false, true),
                NewColumn { default: Some("now()".into()), ..new_column("added", "timestamptz", false, false) },
                new_column("note", "text", true, false),
            ],
        )
        .unwrap();
        assert_eq!(
            sql,
            "CREATE TABLE \"public\".\"order items\" (\n    \"order_id\" bigint,\n    \"line\" int,\n    \"added\" timestamptz DEFAULT now() NOT NULL,\n    \"note\" text,\n    PRIMARY KEY (\"order_id\", \"line\")\n)"
        );
        assert!(create_table_sql("public", "t", &[]).is_err());
        assert!(create_table_sql("public", " ", &[new_column("a", "int", true, false)]).is_err());
        assert!(create_table_sql("public", "t", &[new_column("a", "int", true, false), new_column("a", "text", true, false)]).unwrap_err().contains("twice"));
        assert!(create_table_sql("public", "t", &[new_column("a", "int); DROP TABLE x; --", true, false)]).is_err());
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

/// Tests that need a real server, see `query::server_tests`.
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

        run(StructureChange::AddColumn { name: "score".into(), data_type: "numeric(5,2)".into(), nullable: false, default: Some("0".into()), comment: Some("Points".into()) }).await.unwrap();
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
            &[
                NewColumn { name: "id".into(), data_type: "bigserial".into(), nullable: false, primary_key: true, default: None },
                NewColumn { name: "name".into(), data_type: "text".into(), nullable: false, primary_key: false, default: None },
                NewColumn { name: "joined".into(), data_type: "date".into(), nullable: true, primary_key: false, default: Some("current_date".into()) },
            ],
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
