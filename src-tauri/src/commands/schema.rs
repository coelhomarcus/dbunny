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

    // pg_catalog rather than information_schema, which leaves out materialized views.
    // data_type mimics the standard names the UI relies on ("USER-DEFINED", "ARRAY"...)
    let rows = client
        .query(
            "SELECT a.attname::text AS name, \
             CASE WHEN t.typtype = 'e' OR (t.typtype = 'b' AND tn.nspname <> 'pg_catalog' AND t.typcategory <> 'A') THEN 'USER-DEFINED' \
                  WHEN t.typcategory = 'A' THEN 'ARRAY' \
                  WHEN t.typtype = 'd' THEN format_type(t.typbasetype, NULL) \
                  ELSE format_type(a.atttypid, NULL) END AS data_type, \
             format_type(a.atttypid, a.atttypmod) AS type_name, \
             NOT a.attnotnull AS is_nullable, \
             CASE WHEN COALESCE(to_jsonb(a) ->> 'attgenerated', '') = '' THEN pg_get_expr(d.adbin, d.adrelid) END AS default_value, \
             a.attnum::int AS ordinal_position, \
             COALESCE(a.attnum = ANY(pk.conkey), false) AS is_primary_key, \
             (COALESCE(to_jsonb(a) ->> 'attgenerated', '') <> '' OR COALESCE(to_jsonb(a) ->> 'attidentity', '') = 'a') AS is_generated, \
             col_description(cls.oid, a.attnum) AS comment, \
             (SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder) \
                FROM pg_enum e WHERE e.enumtypid = a.atttypid) AS enum_values \
             FROM pg_class cls \
             JOIN pg_namespace ns ON ns.oid = cls.relnamespace \
             JOIN pg_attribute a ON a.attrelid = cls.oid AND a.attnum > 0 AND NOT a.attisdropped \
             JOIN pg_type t ON t.oid = a.atttypid \
             JOIN pg_namespace tn ON tn.oid = t.typnamespace \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             LEFT JOIN pg_constraint pk ON pk.conrelid = cls.oid AND pk.contype = 'p' \
             WHERE ns.nspname = $1 AND cls.relname = $2 \
             ORDER BY a.attnum",
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

#[tauri::command]
pub async fn get_materialized_views(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    let client = state.client(&session_id).await?;
    let rows = client
        .query(
            "SELECT matviewname::text AS name FROM pg_matviews WHERE schemaname = $1 ORDER BY matviewname",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| TableInfo { name: r.get("name"), schema: schema.clone(), table_type: "matview".into() })
        .collect())
}

/// A sequence, type or other schema object listed in the explorer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObject {
    pub name: String,
    /// For types: "enum", "domain", "composite" or "range".
    pub kind: String,
}

#[tauri::command]
pub async fn get_sequences(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<SchemaObject>, String> {
    let client = state.client(&session_id).await?;
    let rows = client
        .query(
            "SELECT c.relname::text AS name FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind = 'S' AND n.nspname = $1 ORDER BY c.relname",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| SchemaObject { name: r.get("name"), kind: "sequence".into() }).collect())
}

#[tauri::command]
pub async fn get_types(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
) -> Result<Vec<SchemaObject>, String> {
    let client = state.client(&session_id).await?;
    // Composite types made for tables and views (their row types) are left out
    let rows = client
        .query(
            "SELECT t.typname::text AS name, \
             CASE t.typtype WHEN 'e' THEN 'enum' WHEN 'd' THEN 'domain' WHEN 'c' THEN 'composite' WHEN 'r' THEN 'range' END AS kind \
             FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace \
             LEFT JOIN pg_class c ON c.oid = t.typrelid \
             WHERE n.nspname = $1 AND t.typtype IN ('e', 'd', 'c', 'r') \
               AND (t.typtype <> 'c' OR c.relkind = 'c') \
             ORDER BY t.typname",
            &[&schema],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| SchemaObject { name: r.get("name"), kind: r.get("kind") }).collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub schema: String,
    pub comment: Option<String>,
}

#[tauri::command]
pub async fn get_extensions(state: State<'_, AppState>, session_id: String) -> Result<Vec<ExtensionInfo>, String> {
    let client = state.client(&session_id).await?;
    let rows = client
        .query(
            "SELECT e.extname::text AS name, e.extversion AS version, n.nspname::text AS schema, \
             obj_description(e.oid, 'pg_extension') AS comment \
             FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace ORDER BY e.extname",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| ExtensionInfo { name: r.get("name"), version: r.get("version"), schema: r.get("schema"), comment: r.get("comment") })
        .collect())
}

/// What the explorer shows for a sequence or a type: a few facts and its DDL.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDefinition {
    pub kind: String,
    pub facts: Vec<(String, String)>,
    pub comment: Option<String>,
    pub ddl: String,
}

#[tauri::command]
pub async fn get_object_definition(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    name: String,
    kind: String,
) -> Result<ObjectDefinition, String> {
    let client = state.client(&session_id).await?;
    let result = match kind.as_str() {
        "sequence" => sequence_definition(&client, &schema, &name).await,
        "type" => type_definition(&client, &schema, &name).await,
        other => return Err(format!("Unknown object kind \"{other}\"")),
    };
    result.map_err(|e| super::query::db_message(&e))
}

fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", super::tablefilter::quote_ident(schema), super::tablefilter::quote_ident(name))
}

pub(crate) async fn sequence_definition(client: &tokio_postgres::Client, schema: &str, name: &str) -> Result<ObjectDefinition, tokio_postgres::Error> {
    let row = client
        .query_one(
            "SELECT format_type(s.seqtypid, NULL) AS data_type, s.seqstart::text AS start, s.seqincrement::text AS increment, \
             s.seqmin::text AS min, s.seqmax::text AS max, s.seqcache::text AS cache, s.seqcycle AS cycle, \
             (SELECT last_value::text FROM pg_sequences ps WHERE ps.schemaname = n.nspname AND ps.sequencename = c.relname) AS last_value, \
             (SELECT quote_ident(tn.nspname) || '.' || quote_ident(tc.relname) || '.' || quote_ident(a.attname) \
                FROM pg_depend dep JOIN pg_class tc ON tc.oid = dep.refobjid \
                JOIN pg_namespace tn ON tn.oid = tc.relnamespace \
                JOIN pg_attribute a ON a.attrelid = dep.refobjid AND a.attnum = dep.refobjsubid \
                WHERE dep.objid = c.oid AND dep.classid = 'pg_class'::regclass AND dep.deptype IN ('a', 'i') LIMIT 1) AS owned_by, \
             obj_description(c.oid, 'pg_class') AS comment \
             FROM pg_sequence s JOIN pg_class c ON c.oid = s.seqrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &name],
        )
        .await?;
    let get = |col: &str| row.get::<_, String>(col);
    let last_value: Option<String> = row.get("last_value");
    let owned_by: Option<String> = row.get("owned_by");
    let mut ddl = format!(
        "CREATE SEQUENCE {}\n    AS {}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    START WITH {}\n    CACHE {}\n    {}CYCLE;",
        qualified(schema, name),
        get("data_type"),
        get("increment"),
        get("min"),
        get("max"),
        get("start"),
        get("cache"),
        if row.get::<_, bool>("cycle") { "" } else { "NO " },
    );
    if let Some(owner) = &owned_by {
        ddl += &format!("\n\nALTER SEQUENCE {} OWNED BY {owner};", qualified(schema, name));
    }
    let mut facts = vec![
        ("Type".to_string(), get("data_type")),
        ("Last value".to_string(), last_value.unwrap_or_else(|| "not used yet".into())),
        ("Increment".to_string(), get("increment")),
    ];
    if let Some(owner) = owned_by {
        facts.push(("Owned by".to_string(), owner));
    }
    Ok(ObjectDefinition { kind: "sequence".into(), facts, comment: row.get("comment"), ddl })
}

pub(crate) async fn type_definition(client: &tokio_postgres::Client, schema: &str, name: &str) -> Result<ObjectDefinition, tokio_postgres::Error> {
    let row = client
        .query_one(
            "SELECT t.oid, t.typtype::text AS typtype, \
             format_type(t.typbasetype, t.typtypmod) AS base_type, t.typnotnull AS not_null, t.typdefault AS default_value, \
             obj_description(t.oid, 'pg_type') AS comment, \
             (SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid = t.oid) AS labels, \
             ARRAY(SELECT pg_get_constraintdef(con.oid, true) FROM pg_constraint con WHERE con.contypid = t.oid ORDER BY con.conname) AS checks, \
             ARRAY(SELECT quote_ident(a.attname) || ' ' || format_type(a.atttypid, a.atttypmod) FROM pg_attribute a \
                   WHERE a.attrelid = t.typrelid AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum) AS attributes, \
             (SELECT format_type(r.rngsubtype, NULL) FROM pg_range r WHERE r.rngtypid = t.oid) AS subtype \
             FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace \
             WHERE n.nspname = $1 AND t.typname = $2",
            &[&schema, &name],
        )
        .await?;
    let target = qualified(schema, name);
    let typtype: String = row.get("typtype");
    let sql_string = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let (kind, ddl, facts) = match typtype.as_str() {
        "e" => {
            let labels: Vec<String> = row.get::<_, Option<Vec<String>>>("labels").unwrap_or_default();
            let ddl = format!(
                "CREATE TYPE {target} AS ENUM (\n    {}\n);",
                labels.iter().map(|l| sql_string(l)).collect::<Vec<_>>().join(",\n    ")
            );
            ("enum", ddl, vec![("Values".to_string(), labels.len().to_string())])
        }
        "d" => {
            let base: String = row.get("base_type");
            let mut ddl = format!("CREATE DOMAIN {target} AS {base}");
            if let Some(default) = row.get::<_, Option<String>>("default_value") {
                ddl += &format!("\n    DEFAULT {default}");
            }
            if row.get::<_, bool>("not_null") {
                ddl += "\n    NOT NULL";
            }
            for check in row.get::<_, Vec<String>>("checks") {
                ddl += &format!("\n    {check}");
            }
            ("domain", ddl + ";", vec![("Base type".to_string(), base)])
        }
        "r" => {
            let subtype: String = row.get::<_, Option<String>>("subtype").unwrap_or_default();
            ("range", format!("CREATE TYPE {target} AS RANGE (\n    SUBTYPE = {subtype}\n);"), vec![("Subtype".to_string(), subtype)])
        }
        _ => {
            let attributes: Vec<String> = row.get("attributes");
            let ddl = format!("CREATE TYPE {target} AS (\n    {}\n);", attributes.join(",\n    "));
            ("composite", ddl, vec![("Attributes".to_string(), attributes.len().to_string())])
        }
    };
    if let Some(comment) = row.get::<_, Option<String>>("comment") {
        let ddl = format!("{ddl}\n\nCOMMENT ON TYPE {target} IS {};", sql_string(&comment));
        return Ok(ObjectDefinition { kind: kind.into(), facts, comment: Some(comment), ddl });
    }
    Ok(ObjectDefinition { kind: kind.into(), facts, comment: None, ddl })
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

    /// A real schema (materialized views can't be temporary), dropped before and after.
    async fn explorer_fixture(client: &tokio_postgres::Client, schema: &str) {
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE; \
                 CREATE SCHEMA {schema}; \
                 SET search_path = {schema}; \
                 CREATE TYPE level AS ENUM ('low', 'it''s high'); \
                 CREATE DOMAIN email AS text NOT NULL CHECK (VALUE LIKE '%@%'); \
                 CREATE TYPE point2 AS (x int, y int); \
                 CREATE TYPE floatrange AS RANGE (subtype = float8); \
                 COMMENT ON TYPE level IS 'How urgent'; \
                 CREATE TABLE tickets (id serial PRIMARY KEY, level level, contact email, at point2); \
                 CREATE SEQUENCE counter INCREMENT 5 START 100 MAXVALUE 1000 CYCLE; \
                 INSERT INTO tickets (level, contact) VALUES ('low', 'a@x.io'); \
                 CREATE MATERIALIZED VIEW ticket_levels AS SELECT level, count(*) AS total FROM tickets GROUP BY level; \
                 RESET search_path;"
            ))
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn materialized_views_have_columns_and_types_keep_standard_names() {
        let client = connect().await;
        let s = "dbunny_explorer_columns";
        explorer_fixture(&client, s).await;

        let view = table_columns(&client, s, "ticket_levels").await.unwrap();
        let names: Vec<_> = view.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["level", "total"]);
        assert!(view.iter().all(|c| !c.is_primary_key));

        let table = table_columns(&client, s, "tickets").await.unwrap();
        assert_eq!(column(&table, "id").data_type, "integer");
        assert!(column(&table, "id").default_value.as_deref().unwrap().starts_with("nextval("));
        assert_eq!(column(&table, "level").data_type, "USER-DEFINED");
        assert_eq!(column(&table, "contact").data_type, "text", "domains report their base type");

        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn object_definitions_recreate_the_same_objects() {
        let client = connect().await;
        let s = "dbunny_explorer_ddl";
        explorer_fixture(&client, s).await;

        let seq = sequence_definition(&client, s, "counter").await.unwrap();
        assert!(seq.ddl.contains("INCREMENT BY 5") && seq.ddl.contains("START WITH 100") && seq.ddl.contains("\n    CYCLE;"), "{}", seq.ddl);
        let owned = sequence_definition(&client, s, "tickets_id_seq").await.unwrap();
        assert!(owned.ddl.contains(&format!("OWNED BY {s}.tickets.id")), "{}", owned.ddl);
        assert!(owned.facts.iter().any(|(k, v)| k == "Last value" && v == "1"), "{:?}", owned.facts);

        let mut ddls = vec![seq.ddl.clone()];
        for name in ["level", "email", "point2", "floatrange"] {
            let def = type_definition(&client, s, name).await.unwrap();
            ddls.push(def.ddl);
        }
        assert!(ddls[1].contains("'it''s high'") && ddls[1].contains("COMMENT ON TYPE"), "{}", ddls[1]);
        assert!(ddls[2].contains("NOT NULL") && ddls[2].contains("CHECK"), "{}", ddls[2]);

        // Drop everything and run the definitions: they must rebuild identical objects
        client
            .batch_execute(&format!("DROP SCHEMA {s} CASCADE; CREATE SCHEMA {s}"))
            .await
            .unwrap();
        for ddl in &ddls {
            client.batch_execute(ddl).await.unwrap_or_else(|e| panic!("{e:?}\n{ddl}"));
        }
        assert_eq!(type_definition(&client, s, "level").await.unwrap().ddl, ddls[1]);
        assert_eq!(type_definition(&client, s, "email").await.unwrap().ddl, ddls[2]);
        assert_eq!(type_definition(&client, s, "point2").await.unwrap().ddl, ddls[3]);
        assert_eq!(sequence_definition(&client, s, "counter").await.unwrap().ddl, seq.ddl);

        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }
}
