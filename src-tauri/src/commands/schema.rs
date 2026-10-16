use serde::Serialize;
use tauri::State;

use super::state::AppState;

#[derive(Serialize)]
pub struct SchemaInfo {
    pub name: String,
}

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    #[serde(rename = "type")]
    pub table_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    /// The type as Postgres prints it (`varchar(80)`, `mood`, `integer[]`), unlike the
    /// standard `data_type` which says `USER-DEFINED` or `ARRAY` for those.
    pub type_name: String,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub ordinal_position: i32,
    pub is_primary_key: bool,
    /// Generated or `GENERATED ALWAYS AS IDENTITY`: the server computes it, so it can't be written.
    pub is_generated: bool,
    pub comment: Option<String>,
    /// Labels of the column's enum type, in their declared order.
    pub enum_values: Option<Vec<String>>,
    /// The first foreign key that includes this column.
    pub foreign_key: Option<ForeignKeyRef>,
}

/// A foreign key constraint: `columns` of this table point at `ref_columns` of the referenced one.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyRef {
    pub constraint: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub ref_columns: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub schema: String,
    pub return_type: String,
    pub argument_types: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDetail {
    pub name: String,
    pub schema: String,
    pub return_type: String,
    pub argument_types: String,
    pub language: String,
    pub source: String,
    pub kind: String,
    pub volatility: String,
    pub is_strict: bool,
    pub owner: String,
    pub description: Option<String>,
}

#[tauri::command]
pub async fn get_schemas(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<SchemaInfo>, String> {
    let client = state.client(&session_id).await?;

    let rows = client
        .query(
            "SELECT schema_name as name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast') \
             ORDER BY schema_name",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(|r| SchemaInfo { name: r.get("name") }).collect())
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    let client = state.client(&session_id).await?;

    let rows = client
        .query(
            "SELECT table_name as name, table_schema as schema, \
             CASE WHEN table_type = 'BASE TABLE' THEN 'table' ELSE 'view' END as type \
             FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| TableInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            table_type: r.get("type"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_views(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    let client = state.client(&session_id).await?;

    let rows = client
        .query(
            "SELECT table_name as name, table_schema as schema, 'view' as type \
             FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'VIEW' \
             ORDER BY table_name",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| TableInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            table_type: r.get("type"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_functions(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<FunctionInfo>, String> {
    let client = state.client(&session_id).await?;

    let rows = client
        .query(
            "SELECT p.proname as name, n.nspname as schema, \
             pg_get_function_result(p.oid) as return_type, \
             pg_get_function_arguments(p.oid) as argument_types \
             FROM pg_proc p \
             JOIN pg_namespace n ON p.pronamespace = n.oid \
             WHERE n.nspname = $1 AND p.prokind IN ('f', 'p') \
             ORDER BY p.proname",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|r| FunctionInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            return_type: r.get("return_type"),
            argument_types: r.get("argument_types"),
        })
        .collect())
}

#[tauri::command]
pub async fn get_function_detail(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    function_name: String,
    argument_types: String,
) -> Result<FunctionDetail, String> {
    let client = state.client(&session_id).await?;

    let row = client
        .query_one(
            "SELECT p.proname as name, \
             n.nspname as schema, \
             pg_get_function_result(p.oid) as return_type, \
             pg_get_function_arguments(p.oid) as argument_types, \
             l.lanname as language, \
             pg_get_functiondef(p.oid) as source, \
             CASE p.prokind \
               WHEN 'f' THEN 'function' \
               WHEN 'p' THEN 'procedure' \
               WHEN 'a' THEN 'aggregate' \
               WHEN 'w' THEN 'window' \
             END as kind, \
             CASE p.provolatile \
               WHEN 'i' THEN 'IMMUTABLE' \
               WHEN 's' THEN 'STABLE' \
               WHEN 'v' THEN 'VOLATILE' \
             END as volatility, \
             p.proisstrict as is_strict, \
             pg_catalog.pg_get_userbyid(p.proowner) as owner, \
             d.description \
             FROM pg_proc p \
             JOIN pg_namespace n ON p.pronamespace = n.oid \
             JOIN pg_language l ON p.prolang = l.oid \
             LEFT JOIN pg_description d ON d.objoid = p.oid AND d.classoid = 'pg_proc'::regclass \
             WHERE n.nspname = $1 AND p.proname = $2 \
               AND pg_get_function_arguments(p.oid) = $3 \
             LIMIT 1",
            &[&schema, &function_name, &argument_types],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(FunctionDetail {
        name: row.get("name"),
        schema: row.get("schema"),
        return_type: row.get("return_type"),
        argument_types: row.get("argument_types"),
        language: row.get("language"),
        source: row.get::<_, Option<String>>("source").unwrap_or_default(),
        kind: row.get("kind"),
        volatility: row.get("volatility"),
        is_strict: row.get("is_strict"),
        owner: row.get("owner"),
        description: row.get("description"),
    })
}

#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<Vec<ColumnInfo>, String> {
    let client = state.client(&session_id).await?;
    table_columns(&client, &schema, &table).await.map_err(|e| e.to_string())
}

pub(crate) async fn table_columns(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>, tokio_postgres::Error> {
    let foreign_keys = table_foreign_keys(client, schema, table).await?;

    let rows = client
        .query(
            "SELECT c.column_name AS name, c.data_type AS data_type, \
             format_type(a.atttypid, a.atttypmod) AS type_name, \
             (c.is_nullable = 'YES') AS is_nullable, \
             c.column_default AS default_value, \
             c.ordinal_position::int AS ordinal_position, \
             COALESCE(a.attnum = ANY(pk.conkey), false) AS is_primary_key, \
             COALESCE(c.is_generated = 'ALWAYS' OR c.identity_generation = 'ALWAYS', false) AS is_generated, \
             col_description(cls.oid, a.attnum) AS comment, \
             (SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder) \
                FROM pg_enum e WHERE e.enumtypid = a.atttypid) AS enum_values \
             FROM information_schema.columns c \
             JOIN pg_namespace ns ON ns.nspname = c.table_schema \
             JOIN pg_class cls ON cls.relnamespace = ns.oid AND cls.relname = c.table_name \
             JOIN pg_attribute a ON a.attrelid = cls.oid AND a.attname = c.column_name \
             LEFT JOIN pg_constraint pk ON pk.conrelid = cls.oid AND pk.contype = 'p' \
             WHERE c.table_schema = $1 AND c.table_name = $2 \
             ORDER BY c.ordinal_position",
            &[&schema, &table],
        )
        .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let name: String = r.get("name");
            let foreign_key = foreign_keys.iter().find(|fk| fk.columns.contains(&name)).cloned();
            ColumnInfo {
                data_type: r.get("data_type"),
                type_name: r.get("type_name"),
                is_nullable: r.get("is_nullable"),
                default_value: r.get("default_value"),
                ordinal_position: r.get("ordinal_position"),
                is_primary_key: r.get("is_primary_key"),
                is_generated: r.get("is_generated"),
                comment: r.get("comment"),
                enum_values: r.get("enum_values"),
                foreign_key,
                name,
            }
        })
        .collect())
}

async fn table_foreign_keys(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
) -> Result<Vec<ForeignKeyRef>, tokio_postgres::Error> {
    // conkey/confkey are parallel arrays of attribute numbers; WITH ORDINALITY keeps their order
    let rows = client
        .query(
            "SELECT con.conname AS constraint_name, fn.nspname AS ref_schema, fc.relname AS ref_table, \
             ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(num, i) \
                   JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num ORDER BY k.i) AS columns, \
             ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(num, i) \
                   JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.num ORDER BY k.i) AS ref_columns \
             FROM pg_constraint con \
             JOIN pg_class c ON c.oid = con.conrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_class fc ON fc.oid = con.confrelid \
             JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
             WHERE con.contype = 'f' AND n.nspname = $1 AND c.relname = $2 \
             ORDER BY con.conname",
            &[&schema, &table],
        )
        .await?;

    Ok(rows
        .iter()
        .map(|r| ForeignKeyRef {
            constraint: r.get("constraint_name"),
            schema: r.get("ref_schema"),
            table: r.get("ref_table"),
            columns: r.get("columns"),
            ref_columns: r.get("ref_columns"),
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRelation {
    pub schema: String,
    pub name: String,
    #[serde(rename = "type")]
    pub relation_type: String,
    pub columns: Vec<CompletionColumn>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionSchema {
    pub current_schema: Option<String>,
    pub relations: Vec<CompletionRelation>,
    pub functions: Vec<FunctionInfo>,
}

/// Everything the SQL editor needs for autocomplete, fetched in a couple of
/// catalog queries instead of one `get_columns` call per table.
#[tauri::command]
pub async fn get_completion_schema(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<CompletionSchema, String> {
    let client = state.client(&session_id).await?;

    let current_schema: Option<String> = client
        .query_one("SELECT current_schema()::text AS name", &[])
        .await
        .map_err(|e| e.to_string())?
        .get("name");

    let rows = client
        .query(
            "SELECT n.nspname AS schema, c.relname AS name, \
             CASE WHEN c.relkind IN ('v', 'm') THEN 'view' ELSE 'table' END AS type, \
             a.attname AS column_name, \
             format_type(a.atttypid, a.atttypmod) AS data_type \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attribute a \
               ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
             WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') \
               AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
               AND n.nspname NOT LIKE 'pg\\_toast%' \
               AND n.nspname NOT LIKE 'pg\\_temp%' \
             ORDER BY n.nspname, c.relname, a.attnum",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut relations: Vec<CompletionRelation> = Vec::new();
    for r in &rows {
        let schema: String = r.get("schema");
        let name: String = r.get("name");
        let is_same = relations
            .last()
            .is_some_and(|last| last.schema == schema && last.name == name);
        if !is_same {
            relations.push(CompletionRelation {
                schema,
                name,
                relation_type: r.get("type"),
                columns: Vec::new(),
            });
        }
        if let Some(column) = r.get::<_, Option<String>>("column_name") {
            relations.last_mut().unwrap().columns.push(CompletionColumn {
                name: column,
                data_type: r.get("data_type"),
            });
        }
    }

    let function_rows = client
        .query(
            "SELECT p.proname AS name, n.nspname AS schema, \
             COALESCE(pg_get_function_result(p.oid), '') AS return_type, \
             pg_get_function_arguments(p.oid) AS argument_types \
             FROM pg_proc p \
             JOIN pg_namespace n ON p.pronamespace = n.oid \
             WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
               AND n.nspname NOT LIKE 'pg\\_toast%' \
               AND n.nspname NOT LIKE 'pg\\_temp%' \
               AND p.prokind IN ('f', 'p') \
             ORDER BY n.nspname, p.proname",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;

    let functions = function_rows
        .iter()
        .map(|r| FunctionInfo {
            name: r.get("name"),
            schema: r.get("schema"),
            return_type: r.get("return_type"),
            argument_types: r.get("argument_types"),
        })
        .collect();

    Ok(CompletionSchema {
        current_schema,
        relations,
        functions,
    })
}

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;

    async fn connect() -> tokio_postgres::Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .expect("could not connect to the test database");
        tokio::spawn(connection);
        client
    }

    async fn temp_schema(client: &tokio_postgres::Client) -> String {
        client
            .query_one("SELECT nspname::text FROM pg_namespace WHERE oid = pg_my_temp_schema()", &[])
            .await
            .unwrap()
            .get(0)
    }

    async fn fixture(client: &tokio_postgres::Client) -> String {
        client
            .batch_execute(
                "CREATE TYPE pg_temp.mood AS ENUM ('sad', 'ok', 'happy'); \
                 CREATE TEMP TABLE regions (code text, country text, PRIMARY KEY (country, code)); \
                 CREATE TEMP TABLE users (id int PRIMARY KEY, name text); \
                 CREATE TEMP TABLE posts ( \
                   id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
                   author_id int REFERENCES users (id), \
                   country text, region text, \
                   feeling pg_temp.mood, \
                   title text NOT NULL, \
                   title_length int GENERATED ALWAYS AS (length(title)) STORED, \
                   FOREIGN KEY (country, region) REFERENCES regions (country, code)); \
                 COMMENT ON COLUMN posts.title IS 'Shown in the feed';",
            )
            .await
            .unwrap();
        temp_schema(client).await
    }

    fn column<'a>(columns: &'a [ColumnInfo], name: &str) -> &'a ColumnInfo {
        columns.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"))
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn columns_know_their_foreign_keys() {
        let client = connect().await;
        let schema = fixture(&client).await;
        let columns = table_columns(&client, &schema, "posts").await.unwrap();

        let author = column(&columns, "author_id").foreign_key.as_ref().unwrap();
        assert_eq!((author.table.as_str(), author.schema.as_str()), ("users", schema.as_str()));
        assert_eq!((author.columns.clone(), author.ref_columns.clone()), (vec!["author_id".to_string()], vec!["id".to_string()]));

        // Both columns of a composite key point at the same constraint, in the declared order
        let region = column(&columns, "region").foreign_key.clone().unwrap();
        assert_eq!(region, column(&columns, "country").foreign_key.clone().unwrap());
        assert_eq!(region.columns, vec!["country", "region"]);
        assert_eq!(region.ref_columns, vec!["country", "code"]);

        assert!(column(&columns, "title").foreign_key.is_none());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn columns_know_enums_generated_values_and_comments() {
        let client = connect().await;
        let schema = fixture(&client).await;
        let columns = table_columns(&client, &schema, "posts").await.unwrap();

        assert_eq!(column(&columns, "feeling").enum_values.clone().unwrap(), vec!["sad", "ok", "happy"]);
        assert!(column(&columns, "title").enum_values.is_none());
        assert_eq!(column(&columns, "feeling").data_type, "USER-DEFINED");
        assert!(column(&columns, "feeling").type_name.ends_with("mood"));
        assert_eq!(column(&columns, "author_id").type_name, "integer");

        assert!(column(&columns, "id").is_generated, "identity always");
        assert!(column(&columns, "title_length").is_generated, "stored generated");
        assert!(!column(&columns, "title").is_generated);

        assert_eq!(column(&columns, "title").comment.as_deref(), Some("Shown in the feed"));
        assert!(column(&columns, "id").is_primary_key);
        assert!(!column(&columns, "author_id").is_primary_key);

        let regions = table_columns(&client, &schema, "regions").await.unwrap();
        assert!(regions.iter().all(|c| c.is_primary_key), "composite primary key");
    }
}
