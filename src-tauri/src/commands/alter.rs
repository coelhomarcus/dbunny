//! Structure changes from the Structure tab: columns and indexes. Names are quoted;
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

        run(StructureChange::DropIndex { name: "t_email_lower".into(), cascade: false }).await.unwrap();
        run(StructureChange::DropColumn { name: "points".into(), cascade: false }).await.unwrap();
        let count: i64 = client.query_one(&format!("SELECT count(*) FROM pg_indexes WHERE schemaname = '{s}'"), &[]).await.unwrap().get(0);
        assert_eq!(count, 1, "only the primary key is left");
        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }
}
