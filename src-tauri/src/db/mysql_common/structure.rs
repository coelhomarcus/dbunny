//! Everything the Structure tab shows about a table or view, in one call.

use std::collections::BTreeMap;

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Params, Row};

use super::catalog::{non_empty, number, qualified, quote, show_create, string, table_columns, terminated, text};
use crate::db::mysql_common::flavor::Flavor;
use crate::db::mysql_common::version::ServerVersion;
use crate::db::model::{
    ConstraintInfo, ForeignKeyInfo, IndexInfo, ReferencedBy, TableMeta, TablePrivilege, TableStructure, TriggerInfo,
};
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::mysql_common::errors::error_message;

/// A section that failed becomes a warning and an empty value.
fn section<T: Default>(warnings: &mut Vec<String>, endpoint: &MysqlEndpoint, name: &str, result: mysql_async::Result<T>) -> T {
    result.unwrap_or_else(|e| {
        let warning = format!("Couldn't read {name}: {}", error_message(&e, endpoint));
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
        T::default()
    })
}

pub(super) async fn table_structure(
    conn: &mut Conn,
    endpoint: &MysqlEndpoint,
    flavor: &dyn Flavor,
    version: ServerVersion,
    schema: &str,
    table: &str,
) -> Result<TableStructure, String> {
    let fail = |e: mysql_async::Error| error_message(&e, endpoint);
    let row: Row = conn
        .exec_first(
            "SELECT TABLE_TYPE AS table_type, ENGINE AS engine, TABLE_COLLATION AS collation_name, TABLE_ROWS AS table_rows, \
                    DATA_LENGTH AS data_length, INDEX_LENGTH AS index_length, TABLE_COMMENT AS table_comment \
             FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE IN ('BASE TABLE', 'SYSTEM VERSIONED', 'VIEW')",
            (schema, table),
        )
        .await
        .map_err(fail)?
        .ok_or_else(|| format!("{schema}.{table} doesn't exist"))?;
    let is_view = string(&row, "table_type") == "VIEW";

    let mut warnings = Vec::new();
    let meta = if is_view {
        TableMeta { kind: "view".into(), ..TableMeta::default() }
    } else {
        // The statistics of the data dictionary: as fresh as the server keeps them
        let data = number(&row, "data_length");
        let indexes = number(&row, "index_length");
        TableMeta {
            kind: "table".into(),
            comment: non_empty(string(&row, "table_comment")),
            row_estimate: number(&row, "table_rows"),
            table_bytes: data,
            indexes_bytes: indexes,
            total_bytes: data.zip(indexes).map(|(d, i)| d + i),
            engine: text(&row, "engine"),
            collation: text(&row, "collation_name"),
            ..TableMeta::default()
        }
    };
    let columns = section(&mut warnings, endpoint, "the columns", table_columns(conn, flavor, schema, table).await);
    if is_view {
        let ddl = section(&mut warnings, endpoint, "the view definition", show_create(conn, "VIEW", schema, table, 1).await);
        return Ok(TableStructure {
            meta,
            columns,
            indexes: Vec::new(),
            constraints: Vec::new(),
            foreign_keys: Vec::new(),
            referenced_by: Vec::new(),
            triggers: Vec::new(),
            privileges: Vec::new(),
            ddl: terminated(ddl.unwrap_or_default()),
            warnings,
        });
    }

    let indexes = section(&mut warnings, endpoint, "the indexes", table_indexes(conn, flavor, schema, table, version).await);
    let keys = section(
        &mut warnings,
        endpoint,
        "the foreign keys",
        read_foreign_keys(conn, "k.TABLE_SCHEMA = ? AND k.TABLE_NAME = ?", (schema, table)).await,
    );
    let constraints = section(&mut warnings, endpoint, "the constraints", table_constraints(conn, flavor, schema, table, version, &indexes, &keys).await);
    let referencing = section(
        &mut warnings,
        endpoint,
        "the referencing tables",
        read_foreign_keys(conn, "k.REFERENCED_TABLE_SCHEMA = ? AND k.REFERENCED_TABLE_NAME = ?", (schema, table)).await,
    );
    let triggers = section(&mut warnings, endpoint, "the triggers", table_triggers(conn, schema, table).await);
    let privileges = section(&mut warnings, endpoint, "the permissions", table_privileges(conn, schema, table).await);
    let ddl = section(&mut warnings, endpoint, "the definition", show_create(conn, "TABLE", schema, table, 1).await);

    Ok(TableStructure {
        meta,
        columns,
        indexes,
        constraints,
        foreign_keys: keys
            .into_iter()
            .map(|k| ForeignKeyInfo {
                name: k.name,
                columns: k.columns,
                ref_schema: k.ref_schema,
                ref_table: k.ref_table,
                ref_columns: k.ref_columns,
                on_update: k.on_update,
                on_delete: k.on_delete,
            })
            .collect(),
        referenced_by: referencing
            .into_iter()
            .map(|k| ReferencedBy { name: k.name, schema: k.schema, table: k.table, columns: k.columns })
            .collect(),
        triggers,
        privileges,
        ddl: terminated(ddl.unwrap_or_default()),
        warnings,
    })
}

/// A foreign key: `columns` of `schema`.`table` point at `ref_columns` of the referenced table.
pub(super) struct ForeignKey {
    pub schema: String,
    pub table: String,
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

/// The foreign keys whose rows of `KEY_COLUMN_USAGE` (called `k`) `filter` picks, with their
/// rules, ordered by table and name.
pub(super) async fn read_foreign_keys(conn: &mut Conn, filter: &str, params: impl Into<Params> + Send) -> mysql_async::Result<Vec<ForeignKey>> {
    let sql = format!(
        "SELECT k.TABLE_SCHEMA AS table_schema, k.TABLE_NAME AS table_name, k.CONSTRAINT_NAME AS constraint_name, \
                k.COLUMN_NAME AS column_name, k.REFERENCED_TABLE_SCHEMA AS ref_schema, k.REFERENCED_TABLE_NAME AS ref_table, \
                k.REFERENCED_COLUMN_NAME AS ref_column, r.UPDATE_RULE AS update_rule, r.DELETE_RULE AS delete_rule \
         FROM information_schema.KEY_COLUMN_USAGE k \
         JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
           ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.TABLE_NAME = k.TABLE_NAME AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
         WHERE k.REFERENCED_TABLE_NAME IS NOT NULL AND {filter} \
         ORDER BY k.TABLE_SCHEMA, k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION"
    );
    let rows: Vec<Row> = conn.exec(sql, params).await?;
    let mut keys: Vec<ForeignKey> = Vec::new();
    for row in &rows {
        let (schema, table, name) = (string(row, "table_schema"), string(row, "table_name"), string(row, "constraint_name"));
        let (column, ref_column) = (string(row, "column_name"), string(row, "ref_column"));
        match keys.last_mut() {
            Some(last) if last.schema == schema && last.table == table && last.name == name => {
                last.columns.push(column);
                last.ref_columns.push(ref_column);
            }
            _ => keys.push(ForeignKey {
                schema,
                table,
                name,
                columns: vec![column],
                ref_schema: string(row, "ref_schema"),
                ref_table: string(row, "ref_table"),
                ref_columns: vec![ref_column],
                on_update: string(row, "update_rule"),
                on_delete: string(row, "delete_rule"),
            }),
        }
    }
    Ok(keys)
}

/// The (name, type) of each constraint of a table; the type is `PRIMARY KEY`, `UNIQUE`,
/// `FOREIGN KEY` or `CHECK` (the last from MySQL 8.0.16).
pub(super) async fn read_constraint_types(conn: &mut Conn, schema: &str, table: &str) -> mysql_async::Result<Vec<(String, String)>> {
    let rows: Vec<Row> = conn
        .exec(
            "SELECT CONSTRAINT_NAME AS constraint_name, CONSTRAINT_TYPE AS constraint_type \
             FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY CONSTRAINT_NAME",
            (schema, table),
        )
        .await?;
    Ok(rows.iter().map(|r| (string(r, "constraint_name"), string(r, "constraint_type"))).collect())
}

async fn table_indexes(conn: &mut Conn, flavor: &dyn Flavor, schema: &str, table: &str, version: ServerVersion) -> mysql_async::Result<Vec<IndexInfo>> {
    let expression = flavor.index_expression_sql(version);
    let visible = flavor.index_visible_sql();
    let rows: Vec<Row> = conn
        .exec(
            format!(
                "SELECT s.INDEX_NAME AS index_name, s.COLUMN_NAME AS column_name, s.SUB_PART AS sub_part, \
                        s.NON_UNIQUE AS non_unique, s.INDEX_TYPE AS index_type, {visible} AS is_visible, \
                        {expression} AS expression, s.COLLATION AS direction \
                 FROM information_schema.STATISTICS s WHERE s.TABLE_SCHEMA = ? AND s.TABLE_NAME = ? \
                 ORDER BY s.INDEX_NAME, s.SEQ_IN_INDEX"
            ),
            (schema, table),
        )
        .await?;
    // (index, shown parts, parts as SQL), in the order of the rows
    struct Building {
        index: IndexInfo,
        parts: Vec<String>,
        visible: bool,
    }
    let mut built: Vec<Building> = Vec::new();
    for row in &rows {
        let name = string(row, "index_name");
        let unique = number(row, "non_unique") == Some(0);
        if built.last().is_none_or(|b| b.index.name != name) {
            let is_primary = name == "PRIMARY";
            built.push(Building {
                index: IndexInfo {
                    name,
                    columns: Vec::new(),
                    method: string(row, "index_type"),
                    is_unique: unique,
                    is_primary,
                    backs_constraint: is_primary || unique,
                    predicate: None,
                    definition: String::new(),
                    bytes: None,
                },
                parts: Vec::new(),
                visible: true,
            });
        }
        let building = built.last_mut().unwrap();
        building.visible &= string(row, "is_visible") != "NO";
        let descending = string(row, "direction") == "D";
        let (shown, sql) = match (text(row, "column_name"), text(row, "expression")) {
            (Some(column), _) => {
                let prefix = number(row, "sub_part").map(|n| format!("({n})")).unwrap_or_default();
                (format!("{column}{prefix}"), format!("{}{prefix}", quote(&column)))
            }
            (None, Some(expression)) => (expression.clone(), format!("({expression})")),
            (None, None) => continue,
        };
        building.index.columns.push(if descending { format!("{shown} DESC") } else { shown });
        building.parts.push(if descending { format!("{sql} DESC") } else { sql });
    }
    let mut indexes: Vec<IndexInfo> = built
        .into_iter()
        .map(|Building { mut index, parts, visible }| {
            let list = parts.join(", ");
            let using = match index.method.as_str() {
                "BTREE" | "FULLTEXT" | "SPATIAL" | "" => String::new(),
                other => format!(" USING {other}"),
            };
            let invisible = if visible { "" } else { " INVISIBLE" };
            index.definition = if index.is_primary {
                format!("PRIMARY KEY ({list}){using}")
            } else {
                let kind = match index.method.as_str() {
                    "FULLTEXT" => "FULLTEXT ",
                    "SPATIAL" => "SPATIAL ",
                    _ if index.is_unique => "UNIQUE ",
                    _ => "",
                };
                format!("CREATE {kind}INDEX {} ON {} ({list}){using}{invisible}", quote(&index.name), qualified(schema, table))
            };
            index
        })
        .collect();
    indexes.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then(a.name.cmp(&b.name)));
    Ok(indexes)
}

async fn table_constraints(
    conn: &mut Conn,
    flavor: &dyn Flavor,
    schema: &str,
    table: &str,
    version: ServerVersion,
    indexes: &[IndexInfo],
    keys: &[ForeignKey],
) -> mysql_async::Result<Vec<ConstraintInfo>> {
    let list = |name: &str| {
        let index = indexes.iter().find(|i| i.name == name);
        index.map(|i| i.columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    };
    let mut constraints = Vec::new();
    for (name, kind) in read_constraint_types(conn, schema, table).await? {
        match kind.as_str() {
            "PRIMARY KEY" => constraints.push(ConstraintInfo {
                name,
                kind: "primary key".into(),
                definition: format!("PRIMARY KEY ({})", list("PRIMARY")),
            }),
            "UNIQUE" => {
                let definition = format!("UNIQUE ({})", list(&name));
                constraints.push(ConstraintInfo { name, kind: "unique".into(), definition });
            }
            "FOREIGN KEY" => {
                let Some(key) = keys.iter().find(|k| k.name == name) else { continue };
                let columns = |names: &[String]| names.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
                let mut definition = format!(
                    "FOREIGN KEY ({}) REFERENCES {} ({})",
                    columns(&key.columns),
                    qualified(&key.ref_schema, &key.ref_table),
                    columns(&key.ref_columns)
                );
                if key.on_update != "NO ACTION" {
                    definition += &format!(" ON UPDATE {}", key.on_update);
                }
                if key.on_delete != "NO ACTION" {
                    definition += &format!(" ON DELETE {}", key.on_delete);
                }
                constraints.push(ConstraintInfo { name, kind: "foreign key".into(), definition });
            }
            _ => {}
        }
    }
    if flavor.has_check_constraints(version) {
        let rows: Vec<Row> = conn
            .exec(
                format!("SELECT tc.CONSTRAINT_NAME AS constraint_name, cc.CHECK_CLAUSE AS check_clause, {} AS enforced \
                 FROM information_schema.TABLE_CONSTRAINTS tc \
                 JOIN information_schema.CHECK_CONSTRAINTS cc \
                   ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
                 WHERE tc.TABLE_SCHEMA = ? AND tc.TABLE_NAME = ? AND tc.CONSTRAINT_TYPE = 'CHECK' ORDER BY tc.CONSTRAINT_NAME", flavor.check_enforced_sql()),
                (schema, table),
            )
            .await?;
        for row in &rows {
            let clause = string(row, "check_clause");
            if flavor.is_implicit_check(&string(row, "constraint_name"), &clause) {
                continue;
            }
            let mut definition = if clause.starts_with('(') { format!("CHECK {clause}") } else { format!("CHECK ({clause})") };
            if string(row, "enforced") == "NO" {
                definition += " NOT ENFORCED";
            }
            constraints.push(ConstraintInfo { name: string(row, "constraint_name"), kind: "check".into(), definition });
        }
    }
    // The order of the other engines: keys, uniques, foreign keys, checks
    let rank = |kind: &str| ["primary key", "unique", "foreign key", "check"].iter().position(|k| *k == kind).unwrap_or(4);
    constraints.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then(a.name.cmp(&b.name)));
    Ok(constraints)
}

async fn table_triggers(conn: &mut Conn, schema: &str, table: &str) -> mysql_async::Result<Vec<TriggerInfo>> {
    let rows: Vec<Row> = conn
        .exec(
            "SELECT TRIGGER_NAME AS trigger_name, ACTION_TIMING AS timing, EVENT_MANIPULATION AS event, \
                    ACTION_ORIENTATION AS orientation, ACTION_STATEMENT AS statement \
             FROM information_schema.TRIGGERS WHERE EVENT_OBJECT_SCHEMA = ? AND EVENT_OBJECT_TABLE = ? ORDER BY TRIGGER_NAME",
            (schema, table),
        )
        .await?;
    let mut triggers = Vec::new();
    for row in &rows {
        let name = string(row, "trigger_name");
        let (timing, event) = (string(row, "timing"), string(row, "event"));
        // The whole statement; without the privilege to read it, what can be put together
        let definition = show_create(conn, "TRIGGER", schema, &name, 2).await.ok().flatten().map(terminated).unwrap_or_else(|| {
            format!(
                "CREATE TRIGGER {} {timing} {event} ON {} FOR EACH ROW {}",
                quote(&name),
                qualified(schema, table),
                string(row, "statement")
            )
        });
        triggers.push(TriggerInfo {
            name,
            timing,
            events: vec![event],
            level: string(row, "orientation"),
            function: String::new(),
            enabled: true,
            definition,
        });
    }
    Ok(triggers)
}

/// `'user'@'host'` as the server writes an account, as `user@host`: the form the permission
/// changes and the Roles page use.
fn plain_account(grantee: &str) -> String {
    match grantee.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')).and_then(|inner| inner.split_once("'@'")) {
        Some((user, host)) => format!("{user}@{host}"),
        None => grantee.to_string(),
    }
}

/// What each account may do on the table: its own grants and the ones on the whole database.
async fn table_privileges(conn: &mut Conn, schema: &str, table: &str) -> mysql_async::Result<Vec<TablePrivilege>> {
    let rows: Vec<Row> = conn
        .exec(
            "SELECT GRANTEE AS grantee, PRIVILEGE_TYPE AS privilege, IS_GRANTABLE AS grantable \
             FROM information_schema.TABLE_PRIVILEGES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
             UNION ALL \
             SELECT GRANTEE, PRIVILEGE_TYPE, IS_GRANTABLE \
             FROM information_schema.SCHEMA_PRIVILEGES WHERE TABLE_SCHEMA = ?",
            (schema, table, schema),
        )
        .await?;
    // account -> privilege -> may grant it on
    let mut accounts: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
    for row in &rows {
        let grantable = string(row, "grantable") == "YES";
        let entry = accounts.entry(plain_account(&string(row, "grantee"))).or_default().entry(string(row, "privilege")).or_default();
        *entry |= grantable;
    }
    Ok(accounts
        .into_iter()
        .map(|(grantee, privileges)| TablePrivilege {
            grantee,
            grantable: privileges.iter().filter(|(_, grantable)| **grantable).map(|(name, _)| name.clone()).collect(),
            privileges: privileges.into_keys().collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use mysql_async::{Error, ServerError};

    #[test]
    fn accounts_are_written_without_quotes() {
        assert_eq!(super::plain_account("'ana'@'%'"), "ana@%");
        assert_eq!(super::plain_account("'ana'@'local.host'"), "ana@local.host");
        assert_eq!(super::plain_account("ana"), "ana");
    }


    use super::*;

    #[test]
    fn a_section_that_fails_becomes_a_warning_once() {
        let denied = || Error::Server(ServerError { code: 1142, message: "SELECT command denied".into(), state: "42000".into() });
        let endpoint = MysqlEndpoint::default();
        let mut warnings = Vec::new();
        let first: Vec<String> = section(&mut warnings, &endpoint, "the permissions", Err(denied()));
        let _: Vec<String> = section(&mut warnings, &endpoint, "the permissions", Err(denied()));
        assert!(first.is_empty());
        assert_eq!(warnings, vec!["Couldn't read the permissions: SELECT command denied (MySQL error 1142)".to_string()]);
        let kept: Vec<i32> = section(&mut warnings, &endpoint, "the indexes", Ok(vec![1]));
        assert_eq!(kept, vec![1]);
        assert_eq!(warnings.len(), 1);
    }
}
