//! Structure changes SQLite makes without rebuilding the table: adding, renaming and
//! dropping columns, creating and dropping indexes, and the explorer's table actions.
//! The rest (types, NOT NULL, defaults, constraints) needs the table rebuilt, which comes
//! later; until then those changes explain why they can't run.
//!
//! Names are quoted; types, defaults and index expressions are SQL the user writes, so
//! they're checked to be a single fragment (no `;`) and the SQL is shown before it runs.
//! "schema" is the name of a database of the connection (`main`, `temp` or an attached one).
//! Writes run on the tables' connection, each in its own `BEGIN IMMEDIATE` transaction.

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};

use super::catalog::{is_word, tokens, Token};
use super::data::{in_transaction, relation, Failure};
use super::SqliteDriver;
use crate::db::model::{NewColumn, StructureChange};
use crate::db::sql::Dialect;
use crate::db::{unsupported, Engine, SchemaEditor};

fn quote(name: &str) -> String {
    Dialect::Sqlite.quote_ident(name)
}

/// A change SQLite can only make by rebuilding the table.
fn rebuild(action: &str) -> Failure {
    Failure::message(format!("{action} needs the table to be rebuilt in SQLite, which is coming in a later version."))
}

/// Something SQLite doesn't have at all.
fn not_in_sqlite(feature: &str) -> Failure {
    Failure::message(unsupported(Engine::Sqlite, feature))
}

fn fragment(label: &str, sql: &str) -> Result<String, Failure> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(Failure::message(format!("The {label} can't be empty")));
    }
    if trimmed.contains(';') {
        return Err(Failure::message(format!("The {label} must be a single expression, without ';'")));
    }
    Ok(trimmed.to_string())
}

fn name(label: &str, value: &str) -> Result<String, Failure> {
    if value.trim().is_empty() {
        return Err(Failure::message(format!("The {label} can't be empty")));
    }
    Ok(quote(value.trim()))
}

fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The name can't be empty".into());
    }
    Ok(())
}

/// The columns of `table` that can be written: no generated or hidden ones.
fn written_columns(c: &Connection, schema: &str, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = c.prepare(&format!("PRAGMA {}.table_xinfo({})", quote(schema), quote(table)))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>("name")?, r.get::<_, i64>("hidden")?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|(_, hidden)| *hidden == 0).map(|(n, _)| n).collect())
}

/// An index item: one of the table's columns (or a plain word) is quoted as a name,
/// anything else is an expression.
fn index_item(columns: &[String], item: &str) -> Result<String, Failure> {
    let item = item.trim();
    let is_column = columns.iter().any(|c| c.eq_ignore_ascii_case(item));
    let is_word = !item.is_empty() && item.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
    if is_column || is_word {
        Ok(quote(item))
    } else {
        Ok(format!("({})", fragment("index expression", item)?))
    }
}

/// What an index item adds to a generated index name, as PostgreSQL names it: the
/// column, the function an expression calls, or `expr`.
fn index_name_part(columns: &[String], item: &str) -> String {
    let item = item.trim();
    if columns.iter().any(|c| c.eq_ignore_ascii_case(item)) {
        return item.to_string();
    }
    let word: String = item.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
    if word.len() == item.len() || (!word.is_empty() && item[word.len()..].trim_start().starts_with('(')) {
        word
    } else {
        "expr".into()
    }
}

/// `base`, or `base1`, `base2`… when the name is taken by another object of `schema`.
fn free_name(c: &Connection, schema: &str, base: &str) -> rusqlite::Result<String> {
    let sql = format!("SELECT count(*) FROM {}.sqlite_master WHERE name = ?1 COLLATE NOCASE", quote(schema));
    let mut n = 0;
    loop {
        let candidate = if n == 0 { base.to_string() } else { format!("{base}{n}") };
        if c.query_row(&sql, [&candidate], |r| r.get::<_, i64>(0))? == 0 {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// The statement a change runs. Only an index without a name reads the database, to
/// pick a free one.
fn change_sql(c: &Connection, schema: &str, table: &str, change: &StructureChange) -> Result<String, Failure> {
    let alter = format!("ALTER TABLE {}", relation(schema, table));
    let sql = match change {
        StructureChange::AddColumn { name: column, data_type, nullable, default, comment } => {
            if comment.as_deref().is_some_and(|c| !c.trim().is_empty()) {
                return Err(not_in_sqlite("Comments"));
            }
            let mut add = format!("{alter} ADD COLUMN {} {}", name("column name", column)?, fragment("type", data_type)?);
            if !nullable {
                add += " NOT NULL";
            }
            if let Some(default) = default.as_deref().filter(|d| !d.trim().is_empty()) {
                add += &format!(" DEFAULT {}", fragment("default", default)?);
            }
            add
        }
        StructureChange::RenameColumn { name: column, new_name } => {
            format!("{alter} RENAME COLUMN {} TO {}", name("column name", column)?, name("new name", new_name)?)
        }
        // SQLite has no CASCADE; the screens don't offer it
        StructureChange::DropColumn { name: column, cascade: _ } => format!("{alter} DROP COLUMN {}", name("column name", column)?),
        StructureChange::CreateIndex { name: index, columns: items, unique, method, concurrently } => {
            if *concurrently {
                return Err(not_in_sqlite("Creating an index concurrently"));
            }
            if !matches!(method.as_str(), "" | "btree") {
                return Err(not_in_sqlite(&format!("The {method} index method")));
            }
            if items.is_empty() {
                return Err(Failure::message("Choose at least one column".into()));
            }
            let columns = written_columns(c, schema, table)?;
            let list = items.iter().map(|item| index_item(&columns, item)).collect::<Result<Vec<_>, _>>()?;
            let index = match index.as_deref().filter(|n| !n.trim().is_empty()) {
                Some(given) => name("index name", given)?,
                None => {
                    let parts: Vec<String> = items.iter().map(|item| index_name_part(&columns, item)).collect();
                    quote(&free_name(c, schema, &format!("{}_{}_idx", table.trim(), parts.join("_")))?)
                }
            };
            // The index's database comes before its name; the table can't be qualified
            let create = if *unique { "CREATE UNIQUE INDEX" } else { "CREATE INDEX" };
            format!("{create} {}.{index} ON {} ({})", quote(schema), quote(table), list.join(", "))
        }
        StructureChange::DropIndex { name: index, cascade: _ } => format!("DROP INDEX {}.{}", quote(schema), name("index name", index)?),
        StructureChange::SetType { .. } => return Err(rebuild("Changing a column's type")),
        StructureChange::SetNullable { .. } => return Err(rebuild("Changing NOT NULL")),
        StructureChange::SetDefault { .. } => return Err(rebuild("Changing a default")),
        StructureChange::AddForeignKey { .. } => return Err(rebuild("Adding a foreign key")),
        StructureChange::AddUnique { .. } => return Err(rebuild("Adding a unique constraint")),
        StructureChange::AddCheck { .. } => return Err(rebuild("Adding a check constraint")),
        StructureChange::DropConstraint { .. } => return Err(rebuild("Dropping a constraint")),
        StructureChange::SetComment { .. } => return Err(not_in_sqlite("Comments")),
        StructureChange::SetTriggerEnabled { .. } => return Err(not_in_sqlite("Turning a trigger on or off")),
        StructureChange::Grant { .. } | StructureChange::Revoke { .. } => return Err(not_in_sqlite("Permissions")),
    };
    Ok(sql)
}

/// `CREATE TABLE` from the table builder. One key column of type INTEGER stays the rowid
/// (`"id" INTEGER PRIMARY KEY`); any other key is a table-level `PRIMARY KEY (…)`, composite
/// when several columns are marked, and its columns are NOT NULL, since SQLite would
/// otherwise let them hold NULL.
fn create_table_sql(schema: &str, table: &str, columns: &[NewColumn]) -> Result<String, Failure> {
    let target = format!("{}.{}", quote(schema), name("table name", table)?);
    if columns.is_empty() {
        return Err(Failure::message("Add at least one column".into()));
    }
    let keys: Vec<&NewColumn> = columns.iter().filter(|c| c.primary_key).collect();
    let rowid_key = keys.len() == 1 && keys[0].data_type.trim().eq_ignore_ascii_case("INTEGER");
    let mut seen: Vec<String> = Vec::new();
    let mut lines = Vec::new();
    for column in columns {
        let col = name("column name", &column.name)?;
        // SQLite tells column names apart without regard to (ASCII) case
        let folded = column.name.trim().to_ascii_lowercase();
        if seen.contains(&folded) {
            return Err(Failure::message(format!("Column {} appears twice", column.name.trim())));
        }
        seen.push(folded);
        let mut line = format!("{col} {}", fragment("type", &column.data_type)?);
        if column.primary_key && rowid_key {
            line += " PRIMARY KEY";
        } else if !column.nullable || column.primary_key {
            line += " NOT NULL";
        }
        if let Some(default) = column.default.as_deref().filter(|d| !d.trim().is_empty()) {
            line += &format!(" DEFAULT {}", fragment("default", default)?);
        }
        lines.push(line);
    }
    if !keys.is_empty() && !rowid_key {
        let list: Vec<String> = keys.iter().map(|c| quote(c.name.trim())).collect();
        lines.push(format!("PRIMARY KEY ({})", list.join(", ")));
    }
    Ok(format!("CREATE TABLE {target} (\n    {}\n)", lines.join(",\n    ")))
}

/// The `DROP` keyword for an object kind as the explorer names it.
fn keyword(kind: &str) -> Result<&'static str, String> {
    match kind {
        "table" => Ok("TABLE"),
        "view" => Ok("VIEW"),
        "index" => Ok("INDEX"),
        "trigger" => Ok("TRIGGER"),
        other => Err(format!("Unknown kind \"{other}\"")),
    }
}

/// Only tables can be renamed in SQLite.
fn rename_sql(schema: &str, name: &str, kind: &str, new_name: &str) -> Result<String, String> {
    if keyword(kind)? != "TABLE" {
        return Err(unsupported(Engine::Sqlite, &format!("Renaming a {kind}")));
    }
    check_name(new_name)?;
    Ok(format!("ALTER TABLE {} RENAME TO {}", relation(schema, name), quote(new_name)))
}

/// `sql`, a table's `CREATE TABLE`, creating `target` instead: the name after
/// `CREATE [TEMP] TABLE [IF NOT EXISTS]`, quoted in any way and maybe with its database,
/// is swapped. None when `sql` isn't a plain `CREATE TABLE`.
fn renamed_create_table(sql: &str, target: &str) -> Option<String> {
    let all = tokens(sql);
    let mut i = 0;
    let next = |word: &str, i: &mut usize| {
        let found = is_word(all.get(*i), word);
        if found {
            *i += 1;
        }
        found
    };
    if !next("CREATE", &mut i) {
        return None;
    }
    let _ = next("TEMP", &mut i) || next("TEMPORARY", &mut i);
    if !next("TABLE", &mut i) {
        return None;
    }
    if is_word(all.get(i), "IF") && is_word(all.get(i + 1), "NOT") && is_word(all.get(i + 2), "EXISTS") {
        i += 3;
    }
    let is_name = |at: usize| matches!(all.get(at), Some((Token::Word(_) | Token::Quoted, ..)));
    if !is_name(i) {
        return None;
    }
    let (start, mut end) = (all[i].1, all[i].2);
    if matches!(all.get(i + 1), Some((Token::Symbol(b'.'), ..))) && is_name(i + 2) {
        end = all[i + 2].2;
    }
    Some(format!("{}{target}{}", &sql[..start], &sql[end..]))
}

/// A copy of a table with the same columns, defaults and constraints (its `CREATE TABLE`
/// under the new name), optionally with the rows. Indexes and triggers aren't copied.
fn duplicate(c: &Connection, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), Failure> {
    let lookup = format!("SELECT sql FROM {}.sqlite_master WHERE type = 'table' AND name = ?1 COLLATE NOCASE", quote(schema));
    let original: Option<Option<String>> = c.query_row(&lookup, [name], |r| r.get(0)).optional()?;
    let original = original.flatten().ok_or_else(|| Failure::message(format!("{schema}.{name} doesn't exist")))?;
    let target = relation(schema, new_name);
    let create = renamed_create_table(&original, &target)
        .ok_or_else(|| Failure::message(format!("{name} can't be duplicated: only plain tables can")))?;
    // Generated columns can't be written; they're computed again in the copy
    let copy = if with_data {
        let list = written_columns(c, schema, name)?.iter().map(|n| quote(n)).collect::<Vec<_>>().join(", ");
        Some(format!("INSERT INTO {target} ({list}) SELECT {list} FROM {}", relation(schema, name)))
    } else {
        None
    };
    in_transaction(c, "", |c| {
        c.execute_batch(&create)?;
        if let Some(copy) = &copy {
            c.execute_batch(copy)?;
        }
        Ok(())
    })
}

impl SqliteDriver {
    /// Runs one statement the driver built, in its own transaction on the tables' connection.
    async fn write(&self, sql: String) -> Result<(), String> {
        self.on_tables(move |c| in_transaction(c, "", |c| Ok(c.execute_batch(&sql)?))).await
    }
}

#[async_trait]
impl SchemaEditor for SqliteDriver {
    /// The SQL a change would run, for showing before running it.
    async fn preview_change(&self, schema: &str, table: &str, change: &StructureChange) -> Result<String, String> {
        let (schema, table, change) = (schema.to_owned(), table.to_owned(), change.clone());
        let sql = self.on_tables(move |c| change_sql(c, &schema, &table, &change)).await?;
        Ok(format!("{sql};"))
    }

    async fn apply_change(&self, schema: &str, table: &str, change: StructureChange) -> Result<(), String> {
        let (schema, table) = (schema.to_owned(), table.to_owned());
        self.on_tables(move |c| {
            let sql = change_sql(c, &schema, &table, &change)?;
            in_transaction(c, "", |c| Ok(c.execute_batch(&sql)?))
        })
        .await
    }

    async fn preview_create_table(&self, schema: &str, table: &str, columns: &[NewColumn]) -> Result<String, String> {
        let sql = create_table_sql(schema, table, columns).map_err(|f| self.describe(f))?;
        Ok(format!("{sql};"))
    }

    async fn create_table(&self, schema: &str, table: &str, columns: Vec<NewColumn>) -> Result<(), String> {
        let sql = create_table_sql(schema, table, &columns).map_err(|f| self.describe(f))?;
        self.write(sql).await
    }

    async fn rename_relation(&self, schema: &str, name: &str, kind: &str, new_name: &str) -> Result<(), String> {
        self.write(rename_sql(schema, name, kind, new_name)?).await
    }

    /// SQLite has no CASCADE; the screens don't offer it.
    async fn drop_relation(&self, schema: &str, name: &str, kind: &str, _cascade: bool) -> Result<(), String> {
        self.write(format!("DROP {} {}", keyword(kind)?, relation(schema, name))).await
    }

    /// `DELETE FROM`: SQLite has no TRUNCATE, nor identities to restart.
    async fn truncate_table(&self, schema: &str, name: &str, _restart_identity: bool, _cascade: bool) -> Result<(), String> {
        self.write(format!("DELETE FROM {}", relation(schema, name))).await
    }

    async fn duplicate_table(&self, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String> {
        check_name(new_name)?;
        let (schema, name, new_name) = (schema.to_owned(), name.to_owned(), new_name.to_owned());
        self.on_tables(move |c| duplicate(c, &schema, &name, &new_name, with_data)).await
    }

    async fn refresh_materialized_view(&self, _: &str, _: &str) -> Result<(), String> {
        Err(unsupported(Engine::Sqlite, "Refreshing a materialized view"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value as JsonValue};

    use super::super::tests_support::TempDb;
    use super::super::OpenOptions;
    use super::*;
    use crate::db::{Catalog, Driver};

    const HELD: &str = "A transaction in the query editor is holding the database: commit or roll it back, then try again.";

    async fn open(f: &TempDb) -> Arc<SqliteDriver> {
        SqliteDriver::open(f.path(), OpenOptions::default()).await.unwrap()
    }

    fn editor(d: &SqliteDriver) -> &dyn SchemaEditor {
        d.schema_editor().expect("SQLite edits structure")
    }

    /// Every row of `sql`, as the grid would show it.
    async fn query(d: &SqliteDriver, sql: &str) -> Vec<Vec<JsonValue>> {
        let sql = sql.to_string();
        d.tables()
            .run(move |c| {
                let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
                let n = stmt.column_count();
                let rows = stmt
                    .query_map([], |r| (0..n).map(|i| r.get_ref(i).map(super::super::values::value_to_json)).collect())
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<Vec<JsonValue>>, _>>().map_err(|e| e.to_string())
            })
            .await
            .unwrap()
    }

    async fn exec(d: &SqliteDriver, sql: &str) -> Result<(), String> {
        let sql = sql.to_string();
        d.tables().run(move |c| c.execute_batch(&sql).map_err(|e| e.to_string())).await
    }

    /// The SQL SQLite keeps for the object `name` of database `db`.
    async fn stored(d: &SqliteDriver, db: &str, name: &str) -> Option<String> {
        let sql = format!("SELECT sql FROM \"{db}\".sqlite_master WHERE name = '{}'", name.replace('\'', "''"));
        query(d, &sql).await.first().map(|row| row[0].as_str().unwrap().to_string())
    }

    async fn column_names(d: &SqliteDriver, db: &str, table: &str) -> Vec<String> {
        d.columns(db, table).await.unwrap().into_iter().map(|c| c.name).collect()
    }

    fn add_column(name: &str, data_type: &str, nullable: bool, default: Option<&str>) -> StructureChange {
        StructureChange::AddColumn {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default: default.map(String::from),
            comment: None,
        }
    }

    fn index(name: Option<&str>, columns: &[&str], unique: bool) -> StructureChange {
        StructureChange::CreateIndex {
            name: name.map(String::from),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            unique,
            method: String::new(),
            concurrently: false,
        }
    }

    fn new_column(name: &str, data_type: &str, nullable: bool, primary_key: bool) -> NewColumn {
        NewColumn { name: name.into(), data_type: data_type.into(), nullable, primary_key, default: None }
    }

    /// The preview, after checking that applying the change runs without an error.
    async fn preview_then_apply(d: &SqliteDriver, db: &str, table: &str, change: StructureChange) -> String {
        let preview = editor(d).preview_change(db, table, &change).await.unwrap();
        editor(d).apply_change(db, table, change).await.unwrap();
        preview
    }

    /// What SQLite keeps of a `CREATE` the app ran: the text without the database name.
    fn as_kept(preview: &str, db: &str) -> String {
        preview.trim_end_matches(';').replacen(&format!("\"{db}\"."), "", 1)
    }

    #[tokio::test]
    async fn direct_changes_apply_and_preview_the_same_sql() {
        let f = TempDb::with("CREATE TABLE people (id INTEGER PRIMARY KEY, email TEXT); INSERT INTO people VALUES (1, 'A@x.io'), (2, 'b@x.io');");
        let d = open(&f).await;

        // A column on a table with rows: SQLite keeps its definition as written
        let preview = preview_then_apply(&d, "main", "people", add_column("score", "INTEGER", false, Some("0"))).await;
        assert_eq!(preview, "ALTER TABLE \"main\".\"people\" ADD COLUMN \"score\" INTEGER NOT NULL DEFAULT 0;");
        let table = stored(&d, "main", "people").await.unwrap();
        assert_eq!(table, "CREATE TABLE people (id INTEGER PRIMARY KEY, email TEXT, \"score\" INTEGER NOT NULL DEFAULT 0)");
        assert_eq!(query(&d, "SELECT id, score FROM people ORDER BY id").await, vec![vec![json!(1), json!(0)], vec![json!(2), json!(0)]]);

        // An index without a name gets one the way PostgreSQL names it; expressions are kept
        let preview = preview_then_apply(&d, "main", "people", index(None, &["lower(email)", "id"], true)).await;
        assert_eq!(preview, "CREATE UNIQUE INDEX \"main\".\"people_lower_id_idx\" ON \"people\" ((lower(email)), \"id\");");
        assert_eq!(stored(&d, "main", "people_lower_id_idx").await.unwrap(), as_kept(&preview, "main"));
        let err = exec(&d, "INSERT INTO people (id, email) VALUES (1, 'a@X.io')").await.unwrap_err();
        assert!(err.contains("UNIQUE"), "{err}");

        // The same columns again: the next free name; a blank name counts as none
        let preview = preview_then_apply(&d, "main", "people", index(Some(" "), &["lower(email)", "id"], false)).await;
        assert_eq!(preview, "CREATE INDEX \"main\".\"people_lower_id_idx1\" ON \"people\" ((lower(email)), \"id\");");
        assert_eq!(stored(&d, "main", "people_lower_id_idx1").await.unwrap(), as_kept(&preview, "main"));
        let preview = preview_then_apply(&d, "main", "people", index(Some("by_score"), &["score"], false)).await;
        assert_eq!(preview, "CREATE INDEX \"main\".\"by_score\" ON \"people\" (\"score\");");

        let rename = StructureChange::RenameColumn { name: "score".into(), new_name: "points".into() };
        let preview = preview_then_apply(&d, "main", "people", rename).await;
        assert_eq!(preview, "ALTER TABLE \"main\".\"people\" RENAME COLUMN \"score\" TO \"points\";");
        assert_eq!(column_names(&d, "main", "people").await, vec!["id", "email", "points"]);
        assert_eq!(stored(&d, "main", "by_score").await.unwrap(), "CREATE INDEX \"by_score\" ON \"people\" (\"points\")");

        let drop = StructureChange::DropIndex { name: "people_lower_id_idx1".into(), cascade: false };
        let preview = preview_then_apply(&d, "main", "people", drop).await;
        assert_eq!(preview, "DROP INDEX \"main\".\"people_lower_id_idx1\";");
        assert_eq!(stored(&d, "main", "people_lower_id_idx1").await, None);
        let preview = preview_then_apply(&d, "main", "people", StructureChange::DropIndex { name: "by_score".into(), cascade: true }).await;
        assert_eq!(preview, "DROP INDEX \"main\".\"by_score\";", "CASCADE is ignored");

        // Dropping a column keeps the other values
        let drop = StructureChange::DropColumn { name: "points".into(), cascade: true };
        let preview = preview_then_apply(&d, "main", "people", drop).await;
        assert_eq!(preview, "ALTER TABLE \"main\".\"people\" DROP COLUMN \"points\";");
        assert_eq!(column_names(&d, "main", "people").await, vec!["id", "email"]);
        assert_eq!(query(&d, "SELECT id, email FROM people ORDER BY id").await, vec![vec![json!(1), json!("A@x.io")], vec![json!(2), json!("b@x.io")]]);

        // SQLite's own refusal comes through
        let err = editor(&d).apply_change("main", "people", add_column("must", "TEXT", false, None)).await.unwrap_err();
        assert_eq!(err, "Cannot add a NOT NULL column with default value NULL");
        let err = editor(&d).apply_change("main", "people", StructureChange::DropColumn { name: "nope".into(), cascade: false }).await.unwrap_err();
        assert!(err.contains("no such column"), "{err}");
    }

    #[tokio::test]
    async fn rebuild_changes_explain_themselves() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)");
        let d = open(&f).await;
        let rebuild = |action: &str| format!("{action} needs the table to be rebuilt in SQLite, which is coming in a later version.");
        let names = |list: &[&str]| list.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        let cases = vec![
            (StructureChange::SetType { name: "v".into(), data_type: "INTEGER".into(), using: None }, rebuild("Changing a column's type")),
            (StructureChange::SetNullable { name: "v".into(), nullable: false }, rebuild("Changing NOT NULL")),
            (StructureChange::SetDefault { name: "v".into(), default: Some("'x'".into()) }, rebuild("Changing a default")),
            (
                StructureChange::AddForeignKey {
                    name: None,
                    columns: names(&["v"]),
                    ref_schema: "main".into(),
                    ref_table: "t".into(),
                    ref_columns: names(&["id"]),
                    on_update: String::new(),
                    on_delete: String::new(),
                },
                rebuild("Adding a foreign key"),
            ),
            (StructureChange::AddUnique { name: None, columns: names(&["v"]) }, rebuild("Adding a unique constraint")),
            (StructureChange::AddCheck { name: None, expression: "length(v) > 0".into() }, rebuild("Adding a check constraint")),
            (StructureChange::DropConstraint { name: "c".into(), cascade: false }, rebuild("Dropping a constraint")),
            (StructureChange::SetComment { name: "v".into(), comment: Some("x".into()) }, "Comments isn't supported by SQLite".into()),
            (
                StructureChange::AddColumn { name: "n".into(), data_type: "TEXT".into(), nullable: true, default: None, comment: Some("Notes".into()) },
                "Comments isn't supported by SQLite".into(),
            ),
            (StructureChange::SetTriggerEnabled { name: "tr".into(), enabled: false }, "Turning a trigger on or off isn't supported by SQLite".into()),
            (StructureChange::Grant { grantee: "x".into(), privileges: names(&["SELECT"]), grant_option: false }, "Permissions isn't supported by SQLite".into()),
            (StructureChange::Revoke { grantee: "x".into(), privileges: names(&["SELECT"]) }, "Permissions isn't supported by SQLite".into()),
            (
                StructureChange::CreateIndex { name: None, columns: names(&["v"]), unique: false, method: "btree".into(), concurrently: true },
                "Creating an index concurrently isn't supported by SQLite".into(),
            ),
            (
                StructureChange::CreateIndex { name: None, columns: names(&["v"]), unique: false, method: "hash".into(), concurrently: false },
                "The hash index method isn't supported by SQLite".into(),
            ),
            // The same checks as PostgreSQL's on what the user typed
            (add_column("x", "INT; DROP TABLE t", true, None), "The type must be a single expression, without ';'".into()),
            (add_column("x", "INT", true, Some("1; --")), "The default must be a single expression, without ';'".into()),
            (add_column(" ", "INT", true, None), "The column name can't be empty".into()),
            (StructureChange::RenameColumn { name: "v".into(), new_name: " ".into() }, "The new name can't be empty".into()),
            (index(None, &[], false), "Choose at least one column".into()),
            (index(None, &["v); DROP TABLE t; --"], false), "The index expression must be a single expression, without ';'".into()),
        ];
        for (change, message) in cases {
            assert_eq!(editor(&d).preview_change("main", "t", &change).await.unwrap_err(), message, "{change:?}");
            assert_eq!(editor(&d).apply_change("main", "t", change).await.unwrap_err(), message);
        }
        assert_eq!(stored(&d, "main", "t").await.unwrap(), "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)");
        assert_eq!(query(&d, "SELECT count(*) FROM sqlite_master").await, vec![vec![json!(1)]]);
    }

    #[tokio::test]
    async fn create_table_with_a_composite_key() {
        let f = TempDb::with("");
        let d = open(&f).await;
        let columns = vec![
            new_column("order_id", "INTEGER", false, true),
            new_column("line", "INT", true, true),
            NewColumn { default: Some("CURRENT_TIMESTAMP".into()), ..new_column("added", "TEXT", false, false) },
            new_column("note", "TEXT", true, false),
        ];
        let preview = editor(&d).preview_create_table("main", "order items", &columns).await.unwrap();
        assert_eq!(
            preview,
            "CREATE TABLE \"main\".\"order items\" (\n    \"order_id\" INTEGER NOT NULL,\n    \"line\" INT NOT NULL,\n    \"added\" TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,\n    \"note\" TEXT,\n    PRIMARY KEY (\"order_id\", \"line\")\n);"
        );
        editor(&d).create_table("main", "order items", columns).await.unwrap();
        assert_eq!(stored(&d, "main", "order items").await.unwrap(), as_kept(&preview, "main"));

        let keys: Vec<(String, bool)> = d.columns("main", "order items").await.unwrap().into_iter().map(|c| (c.name, c.is_primary_key)).collect();
        let expected = [("order_id", true), ("line", true), ("added", false), ("note", false)];
        assert_eq!(keys, expected.map(|(n, k)| (n.to_string(), k)));
        exec(&d, "INSERT INTO \"order items\" (order_id, line) VALUES (1, 1), (1, 2)").await.unwrap();
        let err = exec(&d, "INSERT INTO \"order items\" (order_id, line) VALUES (1, 1)").await.unwrap_err();
        assert!(err.contains("UNIQUE"), "{err}");
        // A key column of a rowid table could otherwise hold NULL in SQLite
        let err = exec(&d, "INSERT INTO \"order items\" (order_id, line) VALUES (2, NULL)").await.unwrap_err();
        assert!(err.contains("NOT NULL"), "{err}");

        // The same errors as PostgreSQL's, from the preview and from creating
        let bad: Vec<(&str, Vec<NewColumn>, &str)> = vec![
            ("t", vec![], "Add at least one column"),
            (" ", vec![new_column("a", "INT", true, false)], "The table name can't be empty"),
            ("t", vec![new_column(" ", "INT", true, false)], "The column name can't be empty"),
            ("t", vec![new_column("a", " ", true, false)], "The type can't be empty"),
            ("t", vec![new_column("a", "INT", true, false), new_column("A", "TEXT", true, false)], "Column A appears twice"),
            ("t", vec![new_column("a", "INT); DROP TABLE x; --", true, false)], "The type must be a single expression, without ';'"),
        ];
        for (table, columns, message) in bad {
            assert_eq!(editor(&d).preview_create_table("main", table, &columns).await.unwrap_err(), message);
            assert_eq!(editor(&d).create_table("main", table, columns).await.unwrap_err(), message);
        }
        let err = editor(&d).create_table("main", "order items", vec![new_column("a", "INT", true, false)]).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[tokio::test]
    async fn create_table_with_a_rowid_alias_primary_key() {
        let f = TempDb::with("");
        let d = open(&f).await;
        let columns = vec![new_column("id", "integer", false, true), new_column("name", "TEXT", false, false)];
        let preview = editor(&d).preview_create_table("main", "people", &columns).await.unwrap();
        assert_eq!(preview, "CREATE TABLE \"main\".\"people\" (\n    \"id\" integer PRIMARY KEY,\n    \"name\" TEXT NOT NULL\n);");
        editor(&d).create_table("main", "people", columns).await.unwrap();
        exec(&d, "INSERT INTO people (name) VALUES ('ana'), ('bia')").await.unwrap();
        assert_eq!(query(&d, "SELECT id, rowid FROM people ORDER BY id").await, vec![vec![json!(1), json!(1)], vec![json!(2), json!(2)]]);
        let columns = d.columns("main", "people").await.unwrap();
        assert_eq!(columns.iter().map(|c| (c.name.as_str(), c.is_primary_key)).collect::<Vec<_>>(), vec![("id", true), ("name", false)]);

        // Any other type, or several key columns, makes a table-level key
        let columns = vec![new_column("id", "BIGINT", true, true), new_column("name", "TEXT", true, false)];
        let preview = editor(&d).preview_create_table("main", "codes", &columns).await.unwrap();
        assert_eq!(preview, "CREATE TABLE \"main\".\"codes\" (\n    \"id\" BIGINT NOT NULL,\n    \"name\" TEXT,\n    PRIMARY KEY (\"id\")\n);");
    }

    #[tokio::test]
    async fn duplicate_keeps_constraints_and_optionally_rows() {
        let f = TempDb::with(
            "CREATE TABLE [items] (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, qty INTEGER DEFAULT 1 CHECK (qty >= 0),
                 len INTEGER GENERATED ALWAYS AS (length(name)) VIRTUAL);
             INSERT INTO items (name, qty) VALUES ('a', 2), ('bb', 3);
             CREATE INDEX items_qty ON items (qty);
             CREATE TRIGGER items_audit AFTER INSERT ON items BEGIN SELECT 1; END;
             CREATE TABLE parents (id INTEGER PRIMARY KEY);
             CREATE TABLE kids (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parents (id));
             PRAGMA foreign_keys = OFF; INSERT INTO kids VALUES (1, 99);",
        );
        let d = open(&f).await;

        editor(&d).duplicate_table("main", "items", "items copy", true).await.unwrap();
        assert_eq!(
            query(&d, "SELECT id, name, qty, len FROM \"items copy\" ORDER BY id").await,
            vec![vec![json!(1), json!("a"), json!(2), json!(1)], vec![json!(2), json!("bb"), json!(3), json!(2)]]
        );
        assert_eq!(
            stored(&d, "main", "items copy").await.unwrap(),
            "CREATE TABLE \"items copy\" (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, qty INTEGER DEFAULT 1 CHECK (qty >= 0),\n                 len INTEGER GENERATED ALWAYS AS (length(name)) VIRTUAL)"
        );
        // The constraints and defaults hold in the copy
        let err = exec(&d, "INSERT INTO \"items copy\" (name) VALUES ('a')").await.unwrap_err();
        assert!(err.contains("UNIQUE"), "{err}");
        let err = exec(&d, "INSERT INTO \"items copy\" (name, qty) VALUES ('c', -1)").await.unwrap_err();
        assert!(err.contains("CHECK"), "{err}");
        exec(&d, "INSERT INTO \"items copy\" (name) VALUES ('ccc')").await.unwrap();
        assert_eq!(query(&d, "SELECT qty, len FROM \"items copy\" WHERE name = 'ccc'").await, vec![vec![json!(1), json!(3)]]);
        // Indexes and triggers aren't copied
        let extra = "SELECT count(*) FROM sqlite_master WHERE tbl_name = 'items copy' AND type IN ('index', 'trigger') AND sql IS NOT NULL";
        assert_eq!(query(&d, extra).await, vec![vec![json!(0)]]);

        editor(&d).duplicate_table("main", "items", "items_empty", false).await.unwrap();
        assert_eq!(query(&d, "SELECT count(*) FROM items_empty").await, vec![vec![json!(0)]]);

        // A name in use fails and leaves the existing table as it was
        let err = editor(&d).duplicate_table("main", "items", "items copy", true).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(query(&d, "SELECT count(*) FROM \"items copy\"").await, vec![vec![json!(3)]]);

        // Rows the copy refuses undo the copy too
        let err = editor(&d).duplicate_table("main", "kids", "kids_copy", true).await.unwrap_err();
        assert_eq!(err, "FOREIGN KEY constraint failed");
        assert_eq!(stored(&d, "main", "kids_copy").await, None);

        let err = editor(&d).duplicate_table("main", "nope", "x", true).await.unwrap_err();
        assert_eq!(err, "main.nope doesn't exist");
        let err = editor(&d).duplicate_table("main", "items", " ", true).await.unwrap_err();
        assert_eq!(err, "The name can't be empty");
    }

    #[test]
    fn the_copy_takes_the_new_name_in_every_quoting() {
        let target = "\"main\".\"new\"";
        let cases = [
            ("CREATE TABLE t (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE t(a)", "CREATE TABLE \"main\".\"new\"(a)"),
            ("CREATE TABLE \"my \"\"t\"\"\" (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE `t` (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE [t x] (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE 't' (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE main.t (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE \"main\" . [t] (a)", "CREATE TABLE \"main\".\"new\" (a)"),
            ("create table if not exists t (a)", "create table if not exists \"main\".\"new\" (a)"),
            ("CREATE TEMP TABLE t (a)", "CREATE TEMP TABLE \"main\".\"new\" (a)"),
            ("CREATE TABLE /* c */ t -- d\n (a)", "CREATE TABLE /* c */ \"main\".\"new\" -- d\n (a)"),
        ];
        for (original, copy) in cases {
            assert_eq!(renamed_create_table(original, target).as_deref(), Some(copy), "{original}");
        }
        assert_eq!(renamed_create_table("CREATE VIRTUAL TABLE t USING fts5(a)", target), None);
        assert_eq!(renamed_create_table("CREATE TABLE", target), None);
    }

    #[tokio::test]
    async fn views_cannot_be_renamed() {
        let f = TempDb::with(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); CREATE VIEW tv AS SELECT v FROM t;
             CREATE INDEX t_v ON t (v); CREATE TRIGGER t_tr AFTER INSERT ON t BEGIN SELECT 1; END;",
        );
        let d = open(&f).await;
        for (name, kind) in [("tv", "view"), ("t_v", "index"), ("t_tr", "trigger")] {
            let err = editor(&d).rename_relation("main", name, kind, "renamed").await.unwrap_err();
            assert_eq!(err, format!("Renaming a {kind} isn't supported by SQLite"));
        }
        let err = editor(&d).rename_relation("main", "t", "matview", "renamed").await.unwrap_err();
        assert_eq!(err, "Unknown kind \"matview\"");
        let err = editor(&d).rename_relation("main", "t", "table", " ").await.unwrap_err();
        assert_eq!(err, "The name can't be empty");

        // A table renames, and the view, index and trigger follow it
        editor(&d).rename_relation("main", "t", "table", "renamed").await.unwrap();
        assert_eq!(stored(&d, "main", "t").await, None);
        assert_eq!(stored(&d, "main", "t_v").await.unwrap(), "CREATE INDEX t_v ON \"renamed\" (v)");
        exec(&d, "INSERT INTO renamed (v) VALUES ('x')").await.unwrap();
        assert_eq!(query(&d, "SELECT v FROM tv").await, vec![vec![json!("x")]]);
        let err = editor(&d).rename_relation("main", "nope", "table", "x").await.unwrap_err();
        assert_eq!(err, "no such table: main.nope");
    }

    #[tokio::test]
    async fn drops_remove_tables_views_indexes_and_triggers() {
        let f = TempDb::with(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE other (x);
             CREATE VIEW tv AS SELECT v FROM t; CREATE INDEX t_v ON t (v);
             CREATE TRIGGER t_tr AFTER INSERT ON t BEGIN SELECT 1; END;",
        );
        let d = open(&f).await;
        let names = || query(&d, "SELECT name FROM sqlite_master ORDER BY name");
        editor(&d).drop_relation("main", "t_tr", "trigger", false).await.unwrap();
        editor(&d).drop_relation("main", "t_v", "index", true).await.unwrap();
        editor(&d).drop_relation("main", "tv", "view", true).await.unwrap();
        assert_eq!(names().await, vec![vec![json!("other")], vec![json!("t")]]);
        editor(&d).drop_relation("main", "t", "table", true).await.unwrap();
        assert_eq!(names().await, vec![vec![json!("other")]]);

        let err = editor(&d).drop_relation("main", "other", "matview", false).await.unwrap_err();
        assert_eq!(err, "Unknown kind \"matview\"");
        let err = editor(&d).drop_relation("main", "other", "view", false).await.unwrap_err();
        assert_eq!(err, "use DROP TABLE to delete table other");
        let err = editor(&d).drop_relation("main", "nope", "table", false).await.unwrap_err();
        assert_eq!(err, "no such table: main.nope");
    }

    #[tokio::test]
    async fn truncating_deletes_every_row_and_refresh_is_not_supported() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT); INSERT INTO t (v) VALUES ('a'), ('b');");
        let d = open(&f).await;
        editor(&d).truncate_table("main", "t", true, true).await.unwrap();
        assert_eq!(query(&d, "SELECT count(*) FROM t").await, vec![vec![json!(0)]]);
        let err = editor(&d).refresh_materialized_view("main", "t").await.unwrap_err();
        assert_eq!(err, "Refreshing a materialized view isn't supported by SQLite");
    }

    #[tokio::test]
    async fn a_ddl_blocked_by_the_editors_transaction_says_so() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t VALUES (1, 'a');");
        let opts = OpenOptions { busy_timeout: std::time::Duration::from_millis(200), ..Default::default() };
        let d = SqliteDriver::open(f.path(), opts).await.unwrap();
        d.editor().run(|c| c.execute_batch("BEGIN; INSERT INTO t VALUES (2, 'b');").map_err(|e| e.to_string())).await.unwrap();

        let err = editor(&d).apply_change("main", "t", add_column("n", "INTEGER", true, None)).await.unwrap_err();
        assert_eq!(err, HELD);
        let err = editor(&d).create_table("main", "made", vec![new_column("a", "INT", true, false)]).await.unwrap_err();
        assert_eq!(err, HELD);
        let err = editor(&d).duplicate_table("main", "t", "t2", true).await.unwrap_err();
        assert_eq!(err, HELD);
        let err = editor(&d).drop_relation("main", "t", "table", false).await.unwrap_err();
        assert_eq!(err, HELD);
        // A preview doesn't write, so it doesn't wait
        assert!(editor(&d).preview_change("main", "t", &index(None, &["v"], false)).await.is_ok());

        d.editor().run(|c| c.execute_batch("ROLLBACK").map_err(|e| e.to_string())).await.unwrap();
        editor(&d).apply_change("main", "t", add_column("n", "INTEGER", true, None)).await.unwrap();
        assert_eq!(column_names(&d, "main", "t").await, vec!["id", "v", "n"]);
    }

    #[tokio::test]
    async fn changes_on_an_attached_database_stay_in_it() {
        let f = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)");
        let other = TempDb::with("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t VALUES (1, 'x');");
        let d = open(&f).await;
        let path = other.path().display().to_string();
        d.tables().run(move |c| c.execute("ATTACH ?1 AS other", [path]).map(|_| ()).map_err(|e| e.to_string())).await.unwrap();

        let preview = preview_then_apply(&d, "other", "t", add_column("n", "INTEGER", true, Some("7"))).await;
        assert_eq!(preview, "ALTER TABLE \"other\".\"t\" ADD COLUMN \"n\" INTEGER DEFAULT 7;");
        let preview = preview_then_apply(&d, "other", "t", index(None, &["v"], false)).await;
        assert_eq!(preview, "CREATE INDEX \"other\".\"t_v_idx\" ON \"t\" (\"v\");");
        assert_eq!(stored(&d, "other", "t_v_idx").await.unwrap(), as_kept(&preview, "other"));

        editor(&d).create_table("other", "made", vec![new_column("a", "INTEGER", false, true)]).await.unwrap();
        editor(&d).duplicate_table("other", "t", "t2", true).await.unwrap();
        editor(&d).rename_relation("other", "t2", "table", "t3").await.unwrap();
        assert_eq!(query(&d, "SELECT id, v, n FROM other.t3").await, vec![vec![json!(1), json!("x"), json!(7)]]);
        editor(&d).truncate_table("other", "t3", false, false).await.unwrap();
        assert_eq!(query(&d, "SELECT count(*) FROM other.t3").await, vec![vec![json!(0)]]);
        preview_then_apply(&d, "other", "t", StructureChange::DropIndex { name: "t_v_idx".into(), cascade: false }).await;
        editor(&d).drop_relation("other", "made", "table", false).await.unwrap();

        assert_eq!(query(&d, "SELECT name FROM other.sqlite_master ORDER BY name").await, vec![vec![json!("t")], vec![json!("t3")]]);
        assert_eq!(query(&d, "SELECT name FROM main.sqlite_master").await, vec![vec![json!("t")]], "main is untouched");
        assert_eq!(column_names(&d, "main", "t").await, vec!["id", "v"]);
    }

    #[tokio::test]
    async fn names_needing_quotes_are_quoted() {
        let f = TempDb::with("CREATE TABLE \"my \"\"odd\"\" table\" (id INTEGER PRIMARY KEY, \"full name\" TEXT); INSERT INTO \"my \"\"odd\"\" table\" VALUES (1, 'ana');");
        let d = open(&f).await;
        let t = "my \"odd\" table";

        let preview = preview_then_apply(&d, "main", t, add_column("it's \"new\"", "TEXT", true, Some("'x'"))).await;
        assert_eq!(preview, "ALTER TABLE \"main\".\"my \"\"odd\"\" table\" ADD COLUMN \"it's \"\"new\"\"\" TEXT DEFAULT 'x';");
        let rename = StructureChange::RenameColumn { name: "full name".into(), new_name: "full \"name\"".into() };
        preview_then_apply(&d, "main", t, rename).await;
        // A column whose name isn't a plain word is still a column, not an expression
        let preview = preview_then_apply(&d, "main", t, index(None, &["full \"name\""], true)).await;
        assert_eq!(preview, "CREATE UNIQUE INDEX \"main\".\"my \"\"odd\"\" table_full \"\"name\"\"_idx\" ON \"my \"\"odd\"\" table\" (\"full \"\"name\"\"\");");
        preview_then_apply(&d, "main", t, StructureChange::DropIndex { name: "my \"odd\" table_full \"name\"_idx".into(), cascade: false }).await;
        preview_then_apply(&d, "main", t, StructureChange::DropColumn { name: "it's \"new\"".into(), cascade: false }).await;
        assert_eq!(column_names(&d, "main", t).await, vec!["id", "full \"name\""]);

        editor(&d).duplicate_table("main", t, "copy \"odd\"", true).await.unwrap();
        editor(&d).rename_relation("main", t, "table", "new; DROP TABLE x").await.unwrap();
        editor(&d).create_table("main", "t \"q\"", vec![new_column("c \"q\"", "TEXT", true, true)]).await.unwrap();
        assert_eq!(query(&d, "SELECT \"full \"\"name\"\"\" FROM \"copy \"\"odd\"\"\"").await, vec![vec![json!("ana")]]);
        assert_eq!(query(&d, "SELECT count(*) FROM \"new; DROP TABLE x\"").await, vec![vec![json!(1)]]);
        assert_eq!(column_names(&d, "main", "t \"q\"").await, vec!["c \"q\""]);
        editor(&d).drop_relation("main", "copy \"odd\"", "table", false).await.unwrap();
        assert_eq!(stored(&d, "main", "copy \"odd\"").await, None);
    }
}
