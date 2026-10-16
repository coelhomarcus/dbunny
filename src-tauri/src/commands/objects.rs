//! Changes to whole tables and views from the explorer: rename, duplicate, truncate,
//! drop and refresh; and creating, renaming and dropping schemas and databases.
//! Names are always quoted; nothing typed becomes SQL text.

use tauri::State;
use tokio_postgres::Client;

use crate::db::postgres::data::db_message;
use super::state::AppState;
use crate::db::sql::filter::quote_ident;

/// The SQL keyword for a relation kind as the explorer names it.
fn keyword(kind: &str) -> Result<&'static str, String> {
    match kind {
        "table" => Ok("TABLE"),
        "view" => Ok("VIEW"),
        "matview" => Ok("MATERIALIZED VIEW"),
        other => Err(format!("Unknown kind \"{other}\"")),
    }
}

fn relation(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(name))
}

fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The name can't be empty".into());
    }
    if name.len() > 63 {
        return Err("Names can have at most 63 characters".into());
    }
    Ok(())
}

pub(crate) fn rename_sql(schema: &str, name: &str, kind: &str, new_name: &str) -> Result<String, String> {
    check_name(new_name)?;
    Ok(format!("ALTER {} {} RENAME TO {}", keyword(kind)?, relation(schema, name), quote_ident(new_name)))
}

pub(crate) fn drop_sql(schema: &str, name: &str, kind: &str, cascade: bool) -> Result<String, String> {
    Ok(format!("DROP {} {}{}", keyword(kind)?, relation(schema, name), if cascade { " CASCADE" } else { "" }))
}

pub(crate) fn truncate_sql(schema: &str, name: &str, restart_identity: bool, cascade: bool) -> String {
    format!(
        "TRUNCATE TABLE {}{}{}",
        relation(schema, name),
        if restart_identity { " RESTART IDENTITY" } else { "" },
        if cascade { " CASCADE" } else { "" }
    )
}

async fn run(state: &AppState, session_id: &str, sql: &str) -> Result<(), String> {
    let client = state.postgres(session_id).await?.client().await?;
    client.batch_execute(sql).await.map_err(|e| db_message(&e))
}

#[tauri::command]
pub async fn rename_relation(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
    new_name: String,
) -> Result<(), String> {
    run(&state, &session_id, &rename_sql(&schema, &name, &kind, &new_name)?).await
}

#[tauri::command]
pub async fn drop_relation(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
    cascade: bool,
) -> Result<(), String> {
    run(&state, &session_id, &drop_sql(&schema, &name, &kind, cascade)?).await
}

#[tauri::command]
pub async fn truncate_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    restart_identity: bool,
    cascade: bool,
) -> Result<(), String> {
    run(&state, &session_id, &truncate_sql(&schema, &name, restart_identity, cascade)).await
}

#[tauri::command]
pub async fn refresh_materialized_view(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
) -> Result<(), String> {
    run(&state, &session_id, &format!("REFRESH MATERIALIZED VIEW {}", relation(&schema, &name))).await
}

#[tauri::command]
pub async fn duplicate_table(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    new_name: String,
    with_data: bool,
) -> Result<(), String> {
    let client = state.postgres(&session_id).await?.client().await?;
    duplicate(&client, &schema, &name, &new_name, with_data).await
}

/// A copy with the same columns, defaults, constraints, indexes and comments
/// (`LIKE ... INCLUDING ALL`), optionally with the rows. Runs as one transaction, on a
/// single message so no other command of the session can slip into it.
pub(crate) async fn duplicate(client: &Client, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String> {
    check_name(new_name)?;
    let source = relation(schema, name);
    let target = relation(schema, new_name);
    let mut sql = format!("BEGIN; CREATE TABLE {target} (LIKE {source} INCLUDING ALL);");
    if with_data {
        // Generated columns can't be written; identity columns keep their values
        let columns: Vec<String> = client
            .query(
                "SELECT quote_ident(a.attname) FROM pg_attribute a \
                 JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped \
                   AND COALESCE(to_jsonb(a) ->> 'attgenerated', '') = '' \
                 ORDER BY a.attnum",
                &[&schema, &name],
            )
            .await
            .map_err(|e| db_message(&e))?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let list = columns.join(", ");
        sql += &format!(" INSERT INTO {target} ({list}) OVERRIDING SYSTEM VALUE SELECT {list} FROM {source};");
    }
    sql += " COMMIT;";
    if let Err(e) = client.batch_execute(&sql).await {
        // The failed statement left the transaction open; close it before reporting
        let _ = client.batch_execute("ROLLBACK").await;
        return Err(db_message(&e));
    }
    Ok(())
}

pub(crate) fn create_schema_sql(name: &str) -> Result<String, String> {
    check_name(name)?;
    Ok(format!("CREATE SCHEMA {}", quote_ident(name)))
}

pub(crate) fn rename_schema_sql(name: &str, new_name: &str) -> Result<String, String> {
    check_name(new_name)?;
    Ok(format!("ALTER SCHEMA {} RENAME TO {}", quote_ident(name), quote_ident(new_name)))
}

pub(crate) fn drop_schema_sql(name: &str, cascade: bool) -> String {
    format!("DROP SCHEMA {}{}", quote_ident(name), if cascade { " CASCADE" } else { "" })
}

pub(crate) fn create_database_sql(name: &str) -> Result<String, String> {
    check_name(name)?;
    Ok(format!("CREATE DATABASE {}", quote_ident(name)))
}

/// `force` (PostgreSQL 13+) ends the sessions still connected to it instead of failing.
pub(crate) fn drop_database_sql(name: &str, force: bool) -> String {
    format!("DROP DATABASE {}{}", quote_ident(name), if force { " WITH (FORCE)" } else { "" })
}

#[tauri::command]
pub async fn create_schema(state: State<'_, AppState>, session_id: String, name: String) -> Result<(), String> {
    run(&state, &session_id, &create_schema_sql(&name)?).await
}

#[tauri::command]
pub async fn rename_schema(state: State<'_, AppState>, session_id: String, name: String, new_name: String) -> Result<(), String> {
    run(&state, &session_id, &rename_schema_sql(&name, &new_name)?).await
}

#[tauri::command]
pub async fn drop_schema(state: State<'_, AppState>, session_id: String, name: String, cascade: bool) -> Result<(), String> {
    run(&state, &session_id, &drop_schema_sql(&name, cascade)).await
}

/// CREATE and DROP DATABASE refuse to run in a transaction, so they go alone in one message.
#[tauri::command]
pub async fn create_database(state: State<'_, AppState>, session_id: String, name: String) -> Result<(), String> {
    run(&state, &session_id, &create_database_sql(&name)?).await
}

#[tauri::command]
pub async fn drop_database(state: State<'_, AppState>, session_id: String, name: String, force: bool) -> Result<(), String> {
    run(&state, &session_id, &drop_database_sql(&name, force)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_and_databases_are_quoted() {
        assert_eq!(create_schema_sql("sales data").unwrap(), "CREATE SCHEMA \"sales data\"");
        assert_eq!(rename_schema_sql("a", "b\"c").unwrap(), "ALTER SCHEMA \"a\" RENAME TO \"b\"\"c\"");
        assert_eq!(drop_schema_sql("a", true), "DROP SCHEMA \"a\" CASCADE");
        assert_eq!(create_database_sql("app; DROP").unwrap(), "CREATE DATABASE \"app; DROP\"");
        assert_eq!(drop_database_sql("old", true), "DROP DATABASE \"old\" WITH (FORCE)");
        assert!(create_schema_sql(" ").is_err());
        assert!(create_database_sql(&"d".repeat(64)).is_err());
    }

    #[test]
    fn names_are_quoted_in_every_statement() {
        assert_eq!(
            rename_sql("public", "my \"table\"", "table", "new; DROP").unwrap(),
            "ALTER TABLE \"public\".\"my \"\"table\"\"\" RENAME TO \"new; DROP\""
        );
        assert_eq!(drop_sql("s", "v", "matview", true).unwrap(), "DROP MATERIALIZED VIEW \"s\".\"v\" CASCADE");
        assert_eq!(drop_sql("s", "v", "view", false).unwrap(), "DROP VIEW \"s\".\"v\"");
        assert_eq!(truncate_sql("s", "t", true, false), "TRUNCATE TABLE \"s\".\"t\" RESTART IDENTITY");
    }

    #[test]
    fn bad_kinds_and_names_are_refused() {
        assert!(drop_sql("s", "t", "index", false).is_err());
        assert!(rename_sql("s", "t", "table", "  ").is_err());
        assert!(rename_sql("s", "t", "table", &"x".repeat(64)).is_err());
    }
}

/// Tests that need a real server, see `db::postgres::console::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;

    async fn connect() -> Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    async fn count(client: &Client, sql: &str) -> i64 {
        client.query_one(sql, &[]).await.unwrap().get(0)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn duplicates_copy_structure_and_optionally_rows() {
        let client = connect().await;
        let s = "dbunny_objects_dup";
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}; \
                 CREATE TABLE {s}.items (id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name text NOT NULL UNIQUE, \
                   name_length int GENERATED ALWAYS AS (length(name)) STORED); \
                 INSERT INTO {s}.items (name) VALUES ('a'), ('bb');"
            ))
            .await
            .unwrap();

        duplicate(&client, s, "items", "items_copy", true).await.unwrap();
        assert_eq!(count(&client, &format!("SELECT count(*) FROM {s}.items_copy WHERE name_length = length(name)")).await, 2);
        assert_eq!(count(&client, &format!("SELECT max(id)::bigint FROM {s}.items_copy")).await, 2, "identity values kept");
        let unique: i64 = count(&client, &format!("SELECT count(*) FROM pg_indexes WHERE schemaname = '{s}' AND tablename = 'items_copy'")).await;
        assert_eq!(unique, 2, "primary key and unique indexes copied");

        duplicate(&client, s, "items", "items_empty", false).await.unwrap();
        assert_eq!(count(&client, &format!("SELECT count(*) FROM {s}.items_empty")).await, 0);

        // A name that exists fails, leaves nothing behind, and the connection keeps working
        let err = duplicate(&client, s, "items", "items_copy", true).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(count(&client, "SELECT 1::bigint").await, 1);

        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn rename_truncate_refresh_and_drop_work_on_a_real_server() {
        let client = connect().await;
        let s = "dbunny_objects_ops";
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}; \
                 CREATE TABLE {s}.t (id serial PRIMARY KEY, v int); INSERT INTO {s}.t (v) VALUES (1), (2); \
                 CREATE MATERIALIZED VIEW {s}.mv AS SELECT count(*) AS n FROM {s}.t;"
            ))
            .await
            .unwrap();

        client.batch_execute(&rename_sql(s, "t", "table", "renamed").unwrap()).await.unwrap();
        client.batch_execute(&format!("INSERT INTO {s}.renamed (v) VALUES (3)")).await.unwrap();
        client.batch_execute(&format!("REFRESH MATERIALIZED VIEW {}", relation(s, "mv"))).await.unwrap();
        assert_eq!(count(&client, &format!("SELECT n FROM {s}.mv")).await, 3);

        client.batch_execute(&truncate_sql(s, "renamed", true, false)).await.unwrap();
        client.batch_execute(&format!("INSERT INTO {s}.renamed (v) VALUES (9)")).await.unwrap();
        assert_eq!(count(&client, &format!("SELECT id::bigint FROM {s}.renamed")).await, 1, "identity restarted");

        // The materialized view depends on the table: a plain drop refuses, CASCADE goes through
        let err = client.batch_execute(&drop_sql(s, "renamed", "table", false).unwrap()).await.unwrap_err();
        assert!(db_message(&err).contains("depend"), "{}", db_message(&err));
        client.batch_execute(&drop_sql(s, "renamed", "table", true).unwrap()).await.unwrap();
        assert_eq!(count(&client, &format!("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = '{s}' AND relkind IN ('r', 'm')")).await, 0);

        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn schemas_and_databases_come_and_go() {
        let client = connect().await;
        client.batch_execute("DROP SCHEMA IF EXISTS dbunny_new_schema CASCADE; DROP SCHEMA IF EXISTS dbunny_renamed_schema CASCADE").await.unwrap();
        client.batch_execute(&create_schema_sql("dbunny_new_schema").unwrap()).await.unwrap();
        client.batch_execute("CREATE TABLE dbunny_new_schema.t (id int)").await.unwrap();
        client.batch_execute(&rename_schema_sql("dbunny_new_schema", "dbunny_renamed_schema").unwrap()).await.unwrap();
        let err = client.batch_execute(&drop_schema_sql("dbunny_renamed_schema", false)).await.unwrap_err();
        assert!(db_message(&err).contains("depend"), "{}", db_message(&err));
        client.batch_execute(&drop_schema_sql("dbunny_renamed_schema", true)).await.unwrap();
        assert_eq!(count(&client, "SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'dbunny_%_schema'").await, 0);

        client.batch_execute("DROP DATABASE IF EXISTS dbunny_new_db").await.unwrap();
        client.batch_execute(&create_database_sql("dbunny_new_db").unwrap()).await.unwrap();
        assert_eq!(count(&client, "SELECT count(*) FROM pg_database WHERE datname = 'dbunny_new_db'").await, 1);
        client.batch_execute(&drop_database_sql("dbunny_new_db", true)).await.unwrap();
        assert_eq!(count(&client, "SELECT count(*) FROM pg_database WHERE datname = 'dbunny_new_db'").await, 0);
    }
}
