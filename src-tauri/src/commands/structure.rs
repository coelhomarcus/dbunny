//! Everything the Structure tab shows about a table or view, in one call.

use tauri::State;
use tokio_postgres::Client;

use super::query::db_message;
use super::schema::table_columns;
use super::state::AppState;
use super::tablefilter::quote_ident;
use crate::db::model::{
    ConstraintInfo, ForeignKeyInfo, IndexInfo, ReferencedBy, TableMeta, TablePrivilege, TableStructure, TriggerInfo,
};

#[tauri::command]
pub async fn get_table_structure(
    state: State<'_, AppState>,
    session_id: String,
    schema: String,
    table: String,
) -> Result<TableStructure, String> {
    let client = state.client(&session_id).await?;
    table_structure(&client, &schema, &table).await
}

/// A section that failed becomes a warning and an empty value.
fn section<T: Default>(warnings: &mut Vec<String>, name: &str, result: Result<T, tokio_postgres::Error>) -> T {
    result.unwrap_or_else(|e| {
        warnings.push(format!("Couldn't read {name}: {}", db_message(&e)));
        T::default()
    })
}

pub(crate) async fn table_structure(client: &Client, schema: &str, table: &str) -> Result<TableStructure, String> {
    let oid: u32 = client
        .query_opt(
            "SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &table],
        )
        .await
        .map_err(|e| db_message(&e))?
        .ok_or_else(|| format!("{schema}.{table} doesn't exist"))?
        .get(0);

    let mut warnings = Vec::new();
    let meta = section(&mut warnings, "the table details", table_meta(client, oid).await);
    let columns = section(&mut warnings, "the columns", table_columns(client, schema, table).await);
    let indexes = section(&mut warnings, "the indexes", table_indexes(client, oid).await);
    let constraints = section(&mut warnings, "the constraints", table_constraints(client, oid).await);
    let foreign_keys = section(&mut warnings, "the foreign keys", table_foreign_keys(client, oid).await);
    let referenced_by = section(&mut warnings, "the referencing tables", referencing_tables(client, oid).await);
    let triggers = section(&mut warnings, "the triggers", table_triggers(client, oid).await);
    let privileges = section(&mut warnings, "the permissions", table_privileges(client, oid).await);

    let ddl = if meta.kind == "view" || meta.kind == "materialized view" {
        section(&mut warnings, "the view definition", view_ddl(client, oid, schema, table, &meta.kind).await)
    } else {
        let ddl_columns = section(&mut warnings, "the column definitions", ddl_columns(client, oid).await);
        build_table_ddl(&TableDdl {
            schema,
            table,
            columns: &ddl_columns,
            constraints: &constraints,
            indexes: &indexes,
            triggers: &triggers,
            comment: meta.comment.as_deref(),
            column_comments: &columns
                .iter()
                .filter_map(|c| c.comment.as_deref().map(|comment| (c.name.as_str(), comment)))
                .collect::<Vec<_>>(),
        })
    };

    Ok(TableStructure { meta, columns, indexes, constraints, foreign_keys, referenced_by, triggers, privileges, ddl, warnings })
}

async fn table_meta(client: &Client, oid: u32) -> Result<TableMeta, tokio_postgres::Error> {
    let row = client
        .query_one(
            "SELECT CASE c.relkind WHEN 'r' THEN 'table' WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized view' \
               WHEN 'p' THEN 'partitioned table' WHEN 'f' THEN 'foreign table' ELSE c.relkind::text END AS kind, \
             pg_get_userbyid(c.relowner) AS owner, \
             obj_description(c.oid, 'pg_class') AS comment, \
             CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS row_estimate, \
             CASE WHEN c.relkind IN ('r', 'm', 'p') THEN pg_table_size(c.oid) END AS table_bytes, \
             CASE WHEN c.relkind IN ('r', 'm', 'p') THEN pg_indexes_size(c.oid) END AS indexes_bytes, \
             CASE WHEN c.relkind IN ('r', 'm', 'p') THEN pg_total_relation_size(c.oid) END AS total_bytes \
             FROM pg_class c WHERE c.oid = $1",
            &[&oid],
        )
        .await?;
    Ok(TableMeta {
        kind: row.get("kind"),
        owner: row.get("owner"),
        comment: row.get("comment"),
        // Views have no statistics of their own
        row_estimate: row.get::<_, Option<i64>>("row_estimate").filter(|_| row.get::<_, Option<i64>>("table_bytes").is_some()),
        table_bytes: row.get("table_bytes"),
        indexes_bytes: row.get("indexes_bytes"),
        total_bytes: row.get("total_bytes"),
    })
}

async fn table_indexes(client: &Client, oid: u32) -> Result<Vec<IndexInfo>, tokio_postgres::Error> {
    let rows = client
        .query(
            "SELECT i.relname AS name, am.amname AS method, x.indisunique AS is_unique, x.indisprimary AS is_primary, \
             EXISTS (SELECT 1 FROM pg_constraint con WHERE con.conindid = x.indexrelid AND con.conrelid = x.indrelid) \
               AS backs_constraint, \
             pg_get_expr(x.indpred, x.indrelid, true) AS predicate, \
             pg_get_indexdef(x.indexrelid) AS definition, \
             pg_relation_size(x.indexrelid) AS bytes, \
             ARRAY(SELECT pg_get_indexdef(x.indexrelid, k, true) FROM generate_series(1, x.indnatts) k ORDER BY k) AS columns \
             FROM pg_index x \
             JOIN pg_class i ON i.oid = x.indexrelid \
             JOIN pg_am am ON am.oid = i.relam \
             WHERE x.indrelid = $1 \
             ORDER BY x.indisprimary DESC, i.relname",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| IndexInfo {
            name: r.get("name"),
            columns: r.get("columns"),
            method: r.get("method"),
            is_unique: r.get("is_unique"),
            is_primary: r.get("is_primary"),
            backs_constraint: r.get("backs_constraint"),
            predicate: r.get("predicate"),
            definition: r.get("definition"),
            bytes: r.get("bytes"),
        })
        .collect())
}

async fn table_constraints(client: &Client, oid: u32) -> Result<Vec<ConstraintInfo>, tokio_postgres::Error> {
    // Postgres 18 also lists NOT NULL as constraints ('n'); those are shown on the columns
    let rows = client
        .query(
            "SELECT conname AS name, \
             CASE contype WHEN 'p' THEN 'primary key' WHEN 'u' THEN 'unique' WHEN 'c' THEN 'check' \
               WHEN 'f' THEN 'foreign key' WHEN 'x' THEN 'exclusion' ELSE contype::text END AS kind, \
             pg_get_constraintdef(oid, true) AS definition \
             FROM pg_constraint \
             WHERE conrelid = $1 AND contype IN ('p', 'u', 'c', 'f', 'x') \
             ORDER BY array_position(ARRAY['p', 'u', 'f', 'c', 'x'], contype::text), conname",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ConstraintInfo { name: r.get("name"), kind: r.get("kind"), definition: r.get("definition") })
        .collect())
}

const FK_ACTION: &str = "CASE $ WHEN 'a' THEN 'NO ACTION' WHEN 'r' THEN 'RESTRICT' WHEN 'c' THEN 'CASCADE' \
                         WHEN 'n' THEN 'SET NULL' WHEN 'd' THEN 'SET DEFAULT' END";

async fn table_foreign_keys(client: &Client, oid: u32) -> Result<Vec<ForeignKeyInfo>, tokio_postgres::Error> {
    let sql = format!(
        "SELECT con.conname AS name, fn.nspname AS ref_schema, fc.relname AS ref_table, \
         ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(num, i) \
               JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num ORDER BY k.i) AS columns, \
         ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(num, i) \
               JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.num ORDER BY k.i) AS ref_columns, \
         {} AS on_update, {} AS on_delete \
         FROM pg_constraint con \
         JOIN pg_class fc ON fc.oid = con.confrelid \
         JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
         WHERE con.contype = 'f' AND con.conrelid = $1 \
         ORDER BY con.conname",
        FK_ACTION.replace('$', "con.confupdtype"),
        FK_ACTION.replace('$', "con.confdeltype"),
    );
    let rows = client.query(sql.as_str(), &[&oid]).await?;
    Ok(rows
        .iter()
        .map(|r| ForeignKeyInfo {
            name: r.get("name"),
            columns: r.get("columns"),
            ref_schema: r.get("ref_schema"),
            ref_table: r.get("ref_table"),
            ref_columns: r.get("ref_columns"),
            on_update: r.get("on_update"),
            on_delete: r.get("on_delete"),
        })
        .collect())
}

async fn referencing_tables(client: &Client, oid: u32) -> Result<Vec<ReferencedBy>, tokio_postgres::Error> {
    let rows = client
        .query(
            "SELECT con.conname AS name, n.nspname AS schema, c.relname AS table_name, \
             ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(num, i) \
                   JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.num ORDER BY k.i) AS columns \
             FROM pg_constraint con \
             JOIN pg_class c ON c.oid = con.conrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE con.contype = 'f' AND con.confrelid = $1 \
             ORDER BY n.nspname, c.relname, con.conname",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| ReferencedBy {
            name: r.get("name"),
            schema: r.get("schema"),
            table: r.get("table_name"),
            columns: r.get("columns"),
        })
        .collect())
}

async fn table_triggers(client: &Client, oid: u32) -> Result<Vec<TriggerInfo>, tokio_postgres::Error> {
    // tgtype bits: 1 row, 2 before, 4 insert, 8 delete, 16 update, 32 truncate, 64 instead of
    let rows = client
        .query(
            "SELECT t.tgname AS name, \
             CASE WHEN t.tgtype::int & 2 <> 0 THEN 'BEFORE' WHEN t.tgtype::int & 64 <> 0 THEN 'INSTEAD OF' ELSE 'AFTER' END AS timing, \
             array_remove(ARRAY[ \
               CASE WHEN t.tgtype::int & 4 <> 0 THEN 'INSERT' END, \
               CASE WHEN t.tgtype::int & 16 <> 0 THEN 'UPDATE' END, \
               CASE WHEN t.tgtype::int & 8 <> 0 THEN 'DELETE' END, \
               CASE WHEN t.tgtype::int & 32 <> 0 THEN 'TRUNCATE' END], NULL) AS events, \
             CASE WHEN t.tgtype::int & 1 <> 0 THEN 'ROW' ELSE 'STATEMENT' END AS level, \
             p.oid::regproc::text AS function, \
             t.tgenabled <> 'D' AS enabled, \
             pg_get_triggerdef(t.oid, true) AS definition \
             FROM pg_trigger t JOIN pg_proc p ON p.oid = t.tgfoid \
             WHERE t.tgrelid = $1 AND NOT t.tgisinternal \
             ORDER BY t.tgname",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| TriggerInfo {
            name: r.get("name"),
            timing: r.get("timing"),
            events: r.get("events"),
            level: r.get("level"),
            function: r.get("function"),
            enabled: r.get("enabled"),
            definition: r.get("definition"),
        })
        .collect())
}

async fn table_privileges(client: &Client, oid: u32) -> Result<Vec<TablePrivilege>, tokio_postgres::Error> {
    // relacl is NULL until the first GRANT; acldefault gives the owner's implicit rights then
    let rows = client
        .query(
            "SELECT CASE WHEN a.grantee = 0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END AS grantee, \
             array_agg(a.privilege_type ORDER BY a.privilege_type) AS privileges, \
             array_agg(a.privilege_type ORDER BY a.privilege_type) FILTER (WHERE a.is_grantable) AS grantable \
             FROM pg_class c, aclexplode(COALESCE(c.relacl, acldefault('r', c.relowner))) a \
             WHERE c.oid = $1 \
             GROUP BY a.grantee ORDER BY a.grantee = 0, 1",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| TablePrivilege {
            grantee: r.get("grantee"),
            privileges: r.get("privileges"),
            grantable: r.get::<_, Option<Vec<String>>>("grantable").unwrap_or_default(),
        })
        .collect())
}

/// What `CREATE TABLE` needs per column. Identity and generated flags are read
/// through to_jsonb so servers older than the columns themselves still answer.
#[derive(Default, Clone)]
pub(crate) struct DdlColumn {
    pub name: String,
    pub type_name: String,
    pub not_null: bool,
    pub default_expr: Option<String>,
    /// 'a' (ALWAYS) or 'd' (BY DEFAULT) for identity columns.
    pub identity: Option<String>,
    /// 's' for stored generated columns (their expression is in `default_expr`).
    pub generated: Option<String>,
}

async fn ddl_columns(client: &Client, oid: u32) -> Result<Vec<DdlColumn>, tokio_postgres::Error> {
    let rows = client
        .query(
            "SELECT a.attname::text AS name, format_type(a.atttypid, a.atttypmod) AS type_name, a.attnotnull AS not_null, \
             pg_get_expr(d.adbin, d.adrelid) AS default_expr, \
             NULLIF(to_jsonb(a) ->> 'attidentity', '') AS identity, \
             NULLIF(to_jsonb(a) ->> 'attgenerated', '') AS generated \
             FROM pg_attribute a \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
            &[&oid],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| DdlColumn {
            name: r.get("name"),
            type_name: r.get("type_name"),
            not_null: r.get("not_null"),
            default_expr: r.get("default_expr"),
            identity: r.get("identity"),
            generated: r.get("generated"),
        })
        .collect())
}

async fn view_ddl(client: &Client, oid: u32, schema: &str, table: &str, kind: &str) -> Result<String, tokio_postgres::Error> {
    let definition: String = client.query_one("SELECT pg_get_viewdef($1::oid, true)", &[&oid]).await?.get(0);
    let create = if kind == "materialized view" { "CREATE MATERIALIZED VIEW" } else { "CREATE VIEW" };
    Ok(format!(
        "{create} {}.{} AS\n{}",
        quote_ident(schema),
        quote_ident(table),
        definition.trim_end().trim_end_matches(';').trim_end()
    ) + ";")
}

fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub(crate) struct TableDdl<'a> {
    pub schema: &'a str,
    pub table: &'a str,
    pub columns: &'a [DdlColumn],
    pub constraints: &'a [ConstraintInfo],
    pub indexes: &'a [IndexInfo],
    pub triggers: &'a [TriggerInfo],
    pub comment: Option<&'a str>,
    pub column_comments: &'a [(&'a str, &'a str)],
}

/// Rebuilds the table's DDL (Postgres has no single function for it): the table with
/// its constraints, then indexes that aren't part of a constraint, comments and triggers.
pub(crate) fn build_table_ddl(t: &TableDdl) -> String {
    let target = format!("{}.{}", quote_ident(t.schema), quote_ident(t.table));

    let mut lines: Vec<String> = t
        .columns
        .iter()
        .map(|c| {
            let mut line = format!("{} {}", quote_ident(&c.name), c.type_name);
            match (c.generated.as_deref(), c.identity.as_deref()) {
                (Some("s"), _) => line += &format!(" GENERATED ALWAYS AS ({}) STORED", c.default_expr.as_deref().unwrap_or("")),
                (Some("v"), _) => line += &format!(" GENERATED ALWAYS AS ({}) VIRTUAL", c.default_expr.as_deref().unwrap_or("")),
                (_, Some("a")) => line += " GENERATED ALWAYS AS IDENTITY",
                (_, Some("d")) => line += " GENERATED BY DEFAULT AS IDENTITY",
                _ => {
                    if let Some(default) = &c.default_expr {
                        line += &format!(" DEFAULT {default}");
                    }
                }
            }
            if c.not_null && c.identity.is_none() {
                line += " NOT NULL";
            }
            line
        })
        .collect();
    lines.extend(
        t.constraints
            .iter()
            .map(|c| format!("CONSTRAINT {} {}", quote_ident(&c.name), c.definition)),
    );

    let mut out = format!("CREATE TABLE {target} (\n    {}\n);", lines.join(",\n    "));

    let extra_indexes: Vec<_> = t.indexes.iter().filter(|i| !i.backs_constraint).collect();
    if !extra_indexes.is_empty() {
        out += "\n";
        for index in extra_indexes {
            out += &format!("\n{};", index.definition);
        }
    }

    if t.comment.is_some() || !t.column_comments.is_empty() {
        out += "\n";
        if let Some(comment) = t.comment {
            out += &format!("\nCOMMENT ON TABLE {target} IS {};", sql_string(comment));
        }
        for (column, comment) in t.column_comments {
            out += &format!("\nCOMMENT ON COLUMN {target}.{} IS {};", quote_ident(column), sql_string(comment));
        }
    }

    if !t.triggers.is_empty() {
        out += "\n";
        for trigger in t.triggers {
            out += &format!("\n{};", trigger.definition);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, type_name: &str) -> DdlColumn {
        DdlColumn { name: name.into(), type_name: type_name.into(), ..Default::default() }
    }

    #[test]
    fn builds_columns_constraints_and_comments() {
        let columns = vec![
            DdlColumn { not_null: true, default_expr: Some("nextval('users_id_seq'::regclass)".into()), ..column("id", "integer") },
            DdlColumn { identity: Some("a".into()), not_null: true, ..column("code", "bigint") },
            DdlColumn { generated: Some("s".into()), default_expr: Some("length(name)".into()), ..column("len", "integer") },
            column("Name \"quoted\"", "text"),
        ];
        let constraints = vec![ConstraintInfo { name: "users_pkey".into(), kind: "primary key".into(), definition: "PRIMARY KEY (id)".into() }];
        let ddl = build_table_ddl(&TableDdl {
            schema: "public",
            table: "users",
            columns: &columns,
            constraints: &constraints,
            indexes: &[],
            triggers: &[],
            comment: Some("People's accounts"),
            column_comments: &[("id", "Primary key")],
        });
        assert_eq!(
            ddl,
            "CREATE TABLE \"public\".\"users\" (\n    \
             \"id\" integer DEFAULT nextval('users_id_seq'::regclass) NOT NULL,\n    \
             \"code\" bigint GENERATED ALWAYS AS IDENTITY,\n    \
             \"len\" integer GENERATED ALWAYS AS (length(name)) STORED,\n    \
             \"Name \"\"quoted\"\"\" text,\n    \
             CONSTRAINT \"users_pkey\" PRIMARY KEY (id)\n);\n\n\
             COMMENT ON TABLE \"public\".\"users\" IS 'People''s accounts';\n\
             COMMENT ON COLUMN \"public\".\"users\".\"id\" IS 'Primary key';"
        );
    }

    #[test]
    fn indexes_of_constraints_are_not_repeated() {
        let index = |name: &str, backs: bool| IndexInfo {
            name: name.into(),
            columns: vec![],
            method: "btree".into(),
            is_unique: backs,
            is_primary: backs,
            backs_constraint: backs,
            predicate: None,
            definition: format!("CREATE INDEX {name} ON public.t USING btree (a)"),
            bytes: None,
        };
        let ddl = build_table_ddl(&TableDdl {
            schema: "public",
            table: "t",
            columns: &[column("a", "int")],
            constraints: &[],
            indexes: &[index("t_pkey", true), index("t_a_idx", false)],
            triggers: &[],
            comment: None,
            column_comments: &[],
        });
        assert!(ddl.ends_with(");\n\nCREATE INDEX t_a_idx ON public.t USING btree (a);"), "{ddl}");
        assert!(!ddl.contains("t_pkey"));
    }
}

/// Tests that need a real server, see `query::server_tests`.
#[cfg(test)]
mod server_tests {
    use super::*;

    async fn connect() -> Client {
        let url = std::env::var("DBUNNY_TEST_DATABASE_URL")
            .expect("set DBUNNY_TEST_DATABASE_URL to run the server tests");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .expect("could not connect to the test database");
        tokio::spawn(connection);
        client
    }

    async fn fixture(client: &Client) -> String {
        client
            .batch_execute(
                "CREATE TEMP TABLE authors (id int PRIMARY KEY, email text UNIQUE); \
                 CREATE TEMP TABLE books ( \
                   id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
                   author_id int NOT NULL REFERENCES authors (id) ON DELETE CASCADE, \
                   title text NOT NULL CHECK (length(title) > 0), \
                   price numeric(8,2) DEFAULT 0, \
                   price_cents int GENERATED ALWAYS AS ((price * 100)::int) STORED); \
                 CREATE INDEX books_title_idx ON books (lower(title)) WHERE price > 0; \
                 COMMENT ON TABLE books IS 'Every book we sell'; \
                 COMMENT ON COLUMN books.title IS 'As printed on the cover'; \
                 CREATE FUNCTION pg_temp.touch() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$; \
                 CREATE TRIGGER books_touch BEFORE INSERT OR UPDATE ON books FOR EACH ROW EXECUTE FUNCTION pg_temp.touch(); \
                 INSERT INTO authors VALUES (1, 'a@x.io'); \
                 INSERT INTO books (author_id, title, price) VALUES (1, 'Dune', 9.90); \
                 CREATE TEMP VIEW cheap_books AS SELECT title FROM books WHERE price < 10;",
            )
            .await
            .unwrap();
        client
            .query_one("SELECT nspname::text FROM pg_namespace WHERE oid = pg_my_temp_schema()", &[])
            .await
            .unwrap()
            .get(0)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn reads_indexes_constraints_keys_and_triggers() {
        let client = connect().await;
        let schema = fixture(&client).await;
        let s = table_structure(&client, &schema, "books").await.unwrap();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.meta.kind, "table");
        assert_eq!(s.meta.comment.as_deref(), Some("Every book we sell"));
        assert!(s.meta.total_bytes.unwrap() > 0);

        let names: Vec<_> = s.indexes.iter().map(|i| (i.name.as_str(), i.backs_constraint)).collect();
        assert_eq!(names, vec![("books_pkey", true), ("books_title_idx", false)]);
        let partial = &s.indexes[1];
        assert_eq!(partial.columns, vec!["lower(title)"]);
        assert_eq!(partial.predicate.as_deref(), Some("price > 0::numeric"));

        let kinds: Vec<_> = s.constraints.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["primary key", "foreign key", "check"]);

        let fk = &s.foreign_keys[0];
        assert_eq!((fk.ref_table.as_str(), fk.columns.clone(), fk.ref_columns.clone()), ("authors", vec!["author_id".to_string()], vec!["id".to_string()]));
        assert_eq!((fk.on_update.as_str(), fk.on_delete.as_str()), ("NO ACTION", "CASCADE"));

        let trigger = &s.triggers[0];
        assert_eq!((trigger.timing.as_str(), trigger.level.as_str()), ("BEFORE", "ROW"));
        assert_eq!(trigger.events, vec!["INSERT", "UPDATE"]);
        assert!(trigger.enabled);

        // The owner (the test's own role) has every right, implicitly
        let owner: String = client.query_one("SELECT current_user::text", &[]).await.unwrap().get(0);
        let own = s.privileges.iter().find(|p| p.grantee == owner).expect("owner privileges");
        assert!(own.privileges.contains(&"SELECT".to_string()) && own.privileges.contains(&"DELETE".to_string()));

        let authors = table_structure(&client, &schema, "authors").await.unwrap();
        assert_eq!(authors.referenced_by.len(), 1);
        assert_eq!((authors.referenced_by[0].table.as_str(), authors.referenced_by[0].columns.clone()), ("books", vec!["author_id".to_string()]));
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn the_rebuilt_ddl_recreates_the_table() {
        let client = connect().await;
        let schema = fixture(&client).await;
        let s = table_structure(&client, &schema, "books").await.unwrap();
        assert!(s.ddl.contains("GENERATED ALWAYS AS IDENTITY"), "{}", s.ddl);
        assert!(s.ddl.contains("STORED"), "{}", s.ddl);
        assert!(s.ddl.contains("CREATE TRIGGER books_touch"), "{}", s.ddl);

        // Drop the table and run the DDL: Postgres must accept every statement and give the same table back
        client.batch_execute("DROP VIEW cheap_books; DROP TABLE books;").await.unwrap();
        client.batch_execute(&s.ddl).await.unwrap_or_else(|e| panic!("{}\n---\n{}", db_message(&e), s.ddl));
        let again = table_structure(&client, &schema, "books").await.unwrap();
        assert_eq!(again.ddl, s.ddl);
        assert_eq!(again.indexes.len(), 2);
        assert_eq!(again.constraints.len(), 3);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn views_get_their_definition_and_missing_tables_an_error() {
        let client = connect().await;
        let schema = fixture(&client).await;
        let view = table_structure(&client, &schema, "cheap_books").await.unwrap();
        assert_eq!(view.meta.kind, "view");
        assert!(view.ddl.starts_with("CREATE VIEW"), "{}", view.ddl);
        assert!(view.ddl.ends_with(";") && !view.ddl.ends_with(";;"));
        assert!(view.meta.total_bytes.is_none() && view.meta.row_estimate.is_none());

        let missing = table_structure(&client, &schema, "nope").await.err().unwrap();
        assert!(missing.contains("doesn't exist"), "{missing}");
    }
}
