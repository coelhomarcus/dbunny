//! What a PostgreSQL database holds: the explorer, columns, object definitions, autocomplete.

use async_trait::async_trait;

use super::data::db_message;
use super::structure::table_structure;
use super::PostgresDriver;
use crate::db::model::{
    ColumnInfo, CompletionColumn, CompletionRelation, CompletionSchema, Diagram, DiagramColumn, DiagramRelation,
    DiagramTable, ExplorerObject, ExplorerPartition, ExplorerSchema, ExplorerTree, ExtensionInfo, ForeignKeyRef,
    FunctionDetail, FunctionInfo, ObjectDefinition, TableStructure,
};
use crate::db::Catalog;

/// The user's schemas, for a query on `pg_namespace n`: leaves out the system catalogs and
/// the `pg_toast*` / `pg_temp_*` schemas the server makes for its own use and for each
/// session's temporary tables.
const USER_SCHEMAS: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') \
    AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp%'";

/// `alias` is a `pg_namespace` in the query (`n`, `fn`, `tn`).
fn user_namespace(alias: &str) -> String {
    format!(
        "{alias}.nspname NOT IN ('pg_catalog', 'information_schema') \
         AND {alias}.nspname NOT LIKE 'pg\\_toast%' AND {alias}.nspname NOT LIKE 'pg\\_temp%'"
    )
}

/// Tables the diagram draws: ordinary tables, foreign tables and partitioned parents.
const DIAGRAM_RELATIONS: &str = "c.relkind IN ('r', 'f', 'p') AND NOT c.relispartition";

/// Foreign keys that touch `schema`, then the columns of that schema's tables and of the
/// tables on the other end of those keys.
async fn diagram(client: &tokio_postgres::Client, schema: &str) -> Result<Diagram, tokio_postgres::Error> {
    let from_ns = user_namespace("fn");
    let to_ns = user_namespace("tn");
    let rels = format!(
        "SELECT con.conname AS name, con.conrelid AS from_oid, con.confrelid AS to_oid, \
                fn.nspname AS from_schema, fc.relname AS from_table, \
                tn.nspname AS to_schema, tc.relname AS to_table, \
                ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(num, i) \
                      JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num ORDER BY k.i) AS from_columns, \
                ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(num, i) \
                      JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.num ORDER BY k.i) AS to_columns, \
                con.conkey AS conkey, con.confkey AS confkey \
         FROM pg_constraint con \
         JOIN pg_class fc ON fc.oid = con.conrelid \
         JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
         JOIN pg_class tc ON tc.oid = con.confrelid \
         JOIN pg_namespace tn ON tn.oid = tc.relnamespace \
         WHERE con.contype = 'f' \
           AND fc.relkind IN ('r', 'f', 'p') AND NOT fc.relispartition \
           AND tc.relkind IN ('r', 'f', 'p') AND NOT tc.relispartition \
           AND (fn.nspname = $1 OR tn.nspname = $1) \
           AND {from_ns} AND {to_ns}"
    );
    let relation_rows = client
        .query(
            &format!("SELECT name, from_schema, from_table, to_schema, to_table, from_columns, to_columns FROM ({rels}) rels ORDER BY from_schema, from_table, name"),
            &[&schema],
        )
        .await?;
    let relations = relation_rows
        .iter()
        .map(|r| DiagramRelation {
            name: r.get("name"),
            from_schema: r.get("from_schema"),
            from_table: r.get("from_table"),
            from_columns: r.get("from_columns"),
            to_schema: r.get("to_schema"),
            to_table: r.get("to_table"),
            to_columns: r.get("to_columns"),
        })
        .collect::<Vec<_>>();

    let home = user_namespace("n");
    let column_rows = client
        .query(
            &format!(
                "WITH rels AS ({rels}), \
                 wanted AS ( \
                   SELECT c.oid, n.nspname AS schema, c.relname AS name \
                   FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                   WHERE n.nspname = $1 AND {DIAGRAM_RELATIONS} AND {home} \
                   UNION \
                   SELECT c.oid, n.nspname, c.relname \
                   FROM rels r \
                   JOIN pg_class c ON c.oid = r.from_oid OR c.oid = r.to_oid \
                   JOIN pg_namespace n ON n.oid = c.relnamespace \
                 ) \
                 SELECT w.schema, w.name, a.attname AS column_name, \
                        format_type(a.atttypid, a.atttypmod) AS type_name, a.attnum::int4 AS ordinal, \
                        EXISTS (SELECT 1 FROM pg_constraint pk \
                                WHERE pk.conrelid = w.oid AND pk.contype = 'p' AND a.attnum = ANY (pk.conkey)) AS is_primary_key, \
                        EXISTS (SELECT 1 FROM rels r \
                                WHERE (r.from_oid = w.oid AND a.attnum = ANY (r.conkey)) \
                                   OR (r.to_oid = w.oid AND a.attnum = ANY (r.confkey))) AS is_foreign_key \
                 FROM wanted w \
                 JOIN pg_attribute a ON a.attrelid = w.oid AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY w.schema, w.name, a.attnum"
            ),
            &[&schema],
        )
        .await?;

    let mut tables: Vec<DiagramTable> = Vec::new();
    for row in &column_rows {
        let schema_name: String = row.get("schema");
        let name: String = row.get("name");
        let column = DiagramColumn {
            name: row.get("column_name"),
            type_name: row.get("type_name"),
            is_primary_key: row.get("is_primary_key"),
            is_foreign_key: row.get("is_foreign_key"),
            ordinal: row.get("ordinal"),
        };
        if tables.last().is_some_and(|t| t.schema == schema_name && t.name == name) {
            tables.last_mut().unwrap().columns.push(column);
        } else {
            tables.push(DiagramTable { schema: schema_name, name, columns: vec![column] });
        }
    }
    Ok(Diagram { tables, relations })
}

/// The schemas the explorer lists.
pub(crate) async fn user_schemas(client: &tokio_postgres::Client) -> Result<Vec<String>, tokio_postgres::Error> {
    let rows = client
        .query(&format!("SELECT n.nspname::text FROM pg_namespace n WHERE {USER_SCHEMAS} ORDER BY n.nspname"), &[])
        .await?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}

#[async_trait]
impl Catalog for PostgresDriver {
    async fn explorer(&self) -> Result<ExplorerTree, String> {
        let client = self.client().await?;
        explorer(&client).await.map_err(|e| db_message(&e))
    }

    async fn diagram(&self, schema: &str) -> Result<Diagram, String> {
        let client = self.client().await?;
        diagram(&client, schema).await.map_err(|e| db_message(&e))
    }

    async fn columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let client = self.client().await?;
        table_columns(&client, schema, table).await.map_err(|e| e.to_string())
    }

    async fn relation_kind(&self, schema: &str, name: &str) -> Result<Option<String>, String> {
        let client = self.client().await?;
        relation_kind(&client, schema, name).await.map_err(|e| e.to_string())
    }

    async fn table_structure(&self, schema: &str, table: &str) -> Result<TableStructure, String> {
        let client = self.client().await?;
        table_structure(&client, schema, table).await
    }

    async fn completion_schema(&self) -> Result<CompletionSchema, String> {
        let client = self.client().await?;
        completion_schema(&client).await
    }

    async fn object_definition(&self, schema: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String> {
        let client = self.client().await?;
        object_definition(&client, schema, name, kind).await
    }

    async fn function_detail(&self, schema: &str, name: &str, args: &str) -> Result<FunctionDetail, String> {
        let client = self.client().await?;
        function_detail(&client, schema, name, args).await
    }

    async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String> {
        let client = self.client().await?;
        extensions(&client).await
    }
}

async fn function_detail(
    client: &tokio_postgres::Client,
    schema: &str,
    function_name: &str,
    argument_types: &str,
) -> Result<FunctionDetail, String> {
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

/// What a relation is: "table", "partitioned", "foreign", "view" or "matview"; None when
/// there's no relation by that name.
pub(crate) async fn relation_kind(
    client: &tokio_postgres::Client,
    schema: &str,
    name: &str,
) -> Result<Option<String>, tokio_postgres::Error> {
    let row = client
        .query_opt(
            "SELECT CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'partitioned' WHEN 'f' THEN 'foreign' \
                    WHEN 'v' THEN 'view' WHEN 'm' THEN 'matview' END AS kind \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p', 'f', 'v', 'm')",
            &[&schema, &name],
        )
        .await?;
    Ok(row.map(|r| r.get("kind")))
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

async fn extensions(client: &tokio_postgres::Client) -> Result<Vec<ExtensionInfo>, String> {
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

async fn object_definition(
    client: &tokio_postgres::Client,
    schema: &str,
    name: &str,
    kind: &str,
) -> Result<ObjectDefinition, String> {
    let result = match kind {
        "sequence" => sequence_definition(client, schema, name).await,
        "type" => type_definition(client, schema, name).await,
        other => return Err(format!("Unknown object kind \"{other}\"")),
    };
    result.map_err(|e| db_message(&e))
}

fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", crate::db::sql::filter::quote_ident(schema), crate::db::sql::filter::quote_ident(name))
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
             ARRAY(SELECT pg_get_constraintdef(con.oid, true) FROM pg_constraint con WHERE con.contypid = t.oid AND con.contype <> 'n' ORDER BY con.conname) AS checks, \
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
            // From PostgreSQL 17 the NOT NULL is also a constraint (contype 'n'), left out of `checks`
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

/// Partitions of `parent`, nested, from the flat (parent, partition) list.
fn partitions_of(parent: (&str, &str), all: &[(String, String, String, String, String)]) -> Vec<ExplorerPartition> {
    all.iter()
        .filter(|(ps, pn, ..)| ps == parent.0 && pn == parent.1)
        .map(|(_, _, schema, name, bound)| ExplorerPartition {
            schema: schema.clone(),
            name: name.clone(),
            bound: bound.clone(),
            partitions: partitions_of((schema, name), all),
        })
        .collect()
}

/// Every schema with its objects, in one round trip, so the explorer can filter and
/// count without loading folder by folder. Functions and types that belong to an
/// extension are left out: they're the extension's, not the database's own.
pub(crate) async fn explorer(client: &tokio_postgres::Client) -> Result<ExplorerTree, tokio_postgres::Error> {
    let schemas = user_schemas(client).await?;
    let objects = client
        .query(
            &format!(
                "SELECT n.nspname::text AS schema, c.relname::text AS name, \
                   CASE c.relkind WHEN 'v' THEN 'view' WHEN 'm' THEN 'matview' WHEN 'S' THEN 'sequence' ELSE 'table' END AS kind, \
                   CASE c.relkind WHEN 'p' THEN 'partitioned' WHEN 'f' THEN 'foreign' ELSE '' END AS detail \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind IN ('r', 'p', 'f', 'v', 'm', 'S') AND {USER_SCHEMAS} \
                   AND NOT COALESCE((to_jsonb(c) ->> 'relispartition')::boolean, false) \
                 UNION ALL \
                 SELECT n.nspname, p.proname, \
                   CASE WHEN COALESCE(to_jsonb(p) ->> 'prokind', 'f') = 'p' THEN 'procedure' ELSE 'function' END, \
                   pg_get_function_arguments(p.oid) \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE {USER_SCHEMAS} AND COALESCE(to_jsonb(p) ->> 'prokind', 'f') IN ('f', 'p') \
                   AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = p.oid AND d.classid = 'pg_proc'::regclass AND d.deptype = 'e') \
                 UNION ALL \
                 SELECT n.nspname, t.typname, 'type', \
                   CASE t.typtype WHEN 'e' THEN 'enum' WHEN 'd' THEN 'domain' WHEN 'c' THEN 'composite' WHEN 'r' THEN 'range' END \
                 FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace LEFT JOIN pg_class tc ON tc.oid = t.typrelid \
                 WHERE {USER_SCHEMAS} AND t.typtype IN ('e', 'd', 'c', 'r') AND (t.typtype <> 'c' OR tc.relkind = 'c') \
                   AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = t.oid AND d.classid = 'pg_type'::regclass AND d.deptype = 'e') \
                 ORDER BY 1, 3, 2"
            ),
            &[],
        )
        .await?;
    // (parent schema, parent, schema, partition, bound), for nesting under the parents
    let partitions: Vec<(String, String, String, String, String)> = client
        .query(
            "SELECT pn.nspname::text, pc.relname::text, n.nspname::text, c.relname::text, \
               COALESCE(pg_get_expr(c.relpartbound, c.oid), '') \
             FROM pg_inherits i \
             JOIN pg_class c ON c.oid = i.inhrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_class pc ON pc.oid = i.inhparent JOIN pg_namespace pn ON pn.oid = pc.relnamespace \
             WHERE pc.relkind = 'p' \
             ORDER BY c.relname",
            &[],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)))
        .collect();
    let default_schema: Option<String> = client
        .query_one("SELECT current_schema()::text", &[])
        .await
        .ok()
        .and_then(|r| r.get(0));

    let mut tree: Vec<ExplorerSchema> = schemas
        .into_iter()
        .map(|name| ExplorerSchema { name, objects: Vec::new() })
        .collect();
    for row in &objects {
        let schema: String = row.get("schema");
        if let Some(entry) = tree.iter_mut().find(|s| s.name == schema) {
            let name: String = row.get("name");
            let detail = row.get::<_, Option<String>>("detail").unwrap_or_default();
            let partitions = if detail == "partitioned" { partitions_of((&schema, &name), &partitions) } else { Vec::new() };
            entry.objects.push(ExplorerObject { name, kind: row.get("kind"), detail, partitions });
        }
    }
    Ok(ExplorerTree { schemas: tree, default_schema })
}

/// Everything the SQL editor needs for autocomplete, fetched in a couple of
/// catalog queries instead of one `get_columns` call per table.
async fn completion_schema(client: &tokio_postgres::Client) -> Result<CompletionSchema, String> {
    let current_schema: Option<String> = client
        .query_one("SELECT current_schema()::text AS name", &[])
        .await
        .map_err(|e| e.to_string())?
        .get("name");

    let rows = client
        .query(
            &format!(
                "SELECT n.nspname AS schema, c.relname AS name, \
                 CASE WHEN c.relkind IN ('v', 'm') THEN 'view' ELSE 'table' END AS type, \
                 a.attname AS column_name, \
                 format_type(a.atttypid, a.atttypmod) AS data_type \
                 FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 LEFT JOIN pg_attribute a \
                   ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
                 WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND {USER_SCHEMAS} \
                 ORDER BY n.nspname, c.relname, a.attnum"
            ),
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
            &format!(
                "SELECT p.proname AS name, n.nspname AS schema, \
                 COALESCE(pg_get_function_result(p.oid), '') AS return_type, \
                 pg_get_function_arguments(p.oid) AS argument_types \
                 FROM pg_proc p \
                 JOIN pg_namespace n ON p.pronamespace = n.oid \
                 WHERE {USER_SCHEMAS} AND p.prokind IN ('f', 'p') \
                 ORDER BY n.nspname, p.proname"
            ),
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

/// Tests that need a real server, see `console::server_tests`.
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
        // PostgreSQL 17 also keeps the domain's NOT NULL as a constraint: it is written once
        assert_eq!(ddls[2].matches("NOT NULL").count(), 1, "{}", ddls[2]);

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

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn the_explorer_lists_everything_but_extension_objects() {
        let client = connect().await;
        let s = "dbunny_explorer_tree";
        explorer_fixture(&client, s).await;
        client
            .batch_execute(&format!(
                "CREATE FUNCTION {s}.add(a int, b int) RETURNS int LANGUAGE sql AS 'SELECT a + b'; \
                 CREATE TABLE {s}.events (id int, at date) PARTITION BY RANGE (at); \
                 CREATE TABLE {s}.events_2026 PARTITION OF {s}.events FOR VALUES FROM ('2026-01-01') TO ('2027-01-01'); \
                 CREATE SCHEMA IF NOT EXISTS dbunny_explorer_empty; \
                 CREATE EXTENSION IF NOT EXISTS pgcrypto SCHEMA {s};"
            ))
            .await
            .unwrap();
        let tree = explorer(&client).await.unwrap();
        let schema = tree.schemas.iter().find(|x| x.name == s).unwrap();
        let has = |name: &str, kind: &str| schema.objects.iter().any(|o| o.name == name && o.kind == kind);
        assert!(has("tickets", "table") && has("ticket_levels", "matview") && has("counter", "sequence"));
        assert!(has("level", "type") && has("email", "type") && has("add", "function"));
        assert!(has("events", "table") && !has("events_2026", "table"), "partitions are under their parent");
        let events = schema.objects.iter().find(|o| o.name == "events").unwrap();
        assert_eq!(events.detail, "partitioned");
        assert_eq!(events.partitions.len(), 1);
        assert_eq!(events.partitions[0].name, "events_2026");
        assert!(events.partitions[0].bound.starts_with("FOR VALUES FROM ('2026-01-01')"), "{}", events.partitions[0].bound);
        assert_eq!(schema.objects.iter().find(|o| o.name == "add").unwrap().detail, "a integer, b integer");
        assert!(!has("gen_random_uuid", "function") && !has("digest", "function"), "extension functions are hidden");
        assert!(tree.schemas.iter().any(|x| x.name == "dbunny_explorer_empty" && x.objects.is_empty()), "empty schemas are listed");
        assert!(!tree.schemas.iter().any(|x| x.name == "pg_catalog" || x.name == "information_schema"));
        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE; DROP SCHEMA dbunny_explorer_empty")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn relations_report_their_kind() {
        let client = connect().await;
        let s = "dbunny_relation_kinds";
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}; \
                 CREATE TABLE {s}.plain (id int); \
                 CREATE TABLE {s}.split (at date) PARTITION BY RANGE (at); \
                 CREATE VIEW {s}.recent AS SELECT * FROM {s}.plain; \
                 CREATE MATERIALIZED VIEW {s}.totals AS SELECT count(*) FROM {s}.plain; \
                 CREATE SEQUENCE {s}.counter;"
            ))
            .await
            .unwrap();
        let kind = |name: &'static str| {
            let client = &client;
            async move { relation_kind(client, s, name).await.unwrap() }
        };
        assert_eq!(kind("plain").await.as_deref(), Some("table"));
        assert_eq!(kind("split").await.as_deref(), Some("partitioned"));
        assert_eq!(kind("recent").await.as_deref(), Some("view"));
        assert_eq!(kind("totals").await.as_deref(), Some("matview"));
        assert_eq!(kind("counter").await, None, "a sequence isn't a relation with rows");
        assert_eq!(kind("missing").await, None);
        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE")).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn temporary_schemas_stay_out_of_every_list() {
        let client = connect().await;
        // A temporary table makes the server create this session's pg_temp_N and pg_toast_temp_N
        client.batch_execute("CREATE TEMP TABLE dbunny_scratch (id int)").await.unwrap();
        let internal = |name: &String| name.starts_with("pg_temp") || name.starts_with("pg_toast") || name == "pg_catalog" || name == "information_schema";

        let schemas = user_schemas(&client).await.unwrap();
        assert!(schemas.iter().any(|s| s == "public"), "{schemas:?}");
        assert!(!schemas.iter().any(internal), "{schemas:?}");
        let tree = explorer(&client).await.unwrap();
        let names: Vec<String> = tree.schemas.into_iter().map(|s| s.name).collect();
        assert_eq!(names, schemas, "the sidebar and the explorer list the same schemas");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn diagram_keeps_composite_keys_and_tables_a_key_reaches() {
        let client = connect().await;
        let s = "dbunny_diagram";
        let other = "dbunny_diagram_other";
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {s} CASCADE; DROP SCHEMA IF EXISTS {other} CASCADE; \
                 CREATE SCHEMA {s}; CREATE SCHEMA {other}; \
                 CREATE TABLE {other}.accounts (id int PRIMARY KEY); \
                 CREATE TABLE {other}.lonely (id int PRIMARY KEY); \
                 CREATE TABLE {s}.users (id int PRIMARY KEY, name text); \
                 CREATE TABLE {s}.regions (country text, code text, PRIMARY KEY (country, code)); \
                 CREATE TABLE {s}.posts ( \
                   id int PRIMARY KEY, \
                   author_id int REFERENCES {s}.users (id), \
                   account_id int REFERENCES {other}.accounts (id), \
                   country text, region text, \
                   CONSTRAINT posts_region_fk FOREIGN KEY (country, region) REFERENCES {s}.regions (country, code)); \
                 CREATE TABLE {other}.notes (id int PRIMARY KEY, user_id int REFERENCES {s}.users (id)); \
                 CREATE VIEW {s}.user_names AS SELECT name FROM {s}.users;"
            ))
            .await
            .unwrap();

        let diagram = diagram(&client, s).await.unwrap();
        let names: Vec<String> = diagram.tables.iter().map(|t| format!("{}.{}", t.schema, t.name)).collect();
        for table in ["users", "regions", "posts"] {
            assert!(names.contains(&format!("{s}.{table}")), "{names:?}");
        }
        assert!(names.contains(&format!("{other}.accounts")), "{names:?}");
        assert!(names.contains(&format!("{other}.notes")), "a table that points here is included: {names:?}");
        assert!(!names.iter().any(|n| n.ends_with(".lonely")), "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with(".user_names")), "a view is not a card");

        let posts = diagram.tables.iter().find(|t| t.name == "posts").unwrap();
        let author = posts.columns.iter().find(|c| c.name == "author_id").unwrap();
        assert!(author.is_foreign_key && !author.is_primary_key);
        assert!(posts.columns.iter().any(|c| c.name == "id" && c.is_primary_key));

        let region = diagram.relations.iter().find(|r| r.name == "posts_region_fk").unwrap();
        assert_eq!(region.from_columns, vec!["country".to_string(), "region".to_string()]);
        assert_eq!(region.to_columns, vec!["country".to_string(), "code".to_string()]);
        assert_eq!((region.to_schema.as_str(), region.to_table.as_str()), (s, "regions"));

        let cross = diagram.relations.iter().find(|r| r.to_table == "accounts").unwrap();
        assert_eq!(cross.to_schema, other);
        let incoming = diagram.relations.iter().find(|r| r.from_table == "notes").unwrap();
        assert_eq!((incoming.from_schema.as_str(), incoming.to_table.as_str()), (other, "users"));

        client.batch_execute(&format!("DROP SCHEMA {s} CASCADE; DROP SCHEMA {other} CASCADE")).await.unwrap();
    }
}
