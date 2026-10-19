//! What is MariaDB's own: its version rules, capabilities and system databases.

use async_trait::async_trait;
use mysql_async::prelude::Queryable;

use super::accounts;
use super::plan::tree_from_json;
use super::version::parse_mariadb_version;
use crate::db::model::{ExplainResult, RoleInfo};
use crate::db::mysql_common::admin::RawSession;
use crate::db::mysql_common::catalog::{ColumnDefinition, StoredColumn};
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::mysql_common::errors::error_message;
use crate::db::mysql_common::flavor::Flavor;
use crate::db::mysql_common::version::ServerVersion;
use crate::db::{Capabilities, Engine, ExplainFormat};

pub struct MariadbFlavor;

#[async_trait]
impl Flavor for MariadbFlavor {
    fn engine(&self) -> Engine {
        Engine::Mariadb
    }

    fn parse_version(&self, text: &str) -> Result<ServerVersion, String> {
        parse_mariadb_version(text)
    }

    fn capabilities(&self, version: ServerVersion) -> Capabilities {
        Capabilities::mariadb(version)
    }

    fn system_databases(&self) -> &'static [&'static str] {
        &["mysql", "sys", "information_schema", "performance_schema"]
    }

    /// MariaDB marks the indexes the optimizer ignores instead of the visible ones.
    fn index_visible_sql(&self) -> &'static str {
        "IF(s.IGNORED = 'YES', 'NO', 'YES')"
    }

    /// MariaDB has no functional key parts.
    fn index_expression_sql(&self, _version: ServerVersion) -> &'static str {
        "NULL"
    }

    fn has_check_constraints(&self, _version: ServerVersion) -> bool {
        true
    }

    /// MariaDB doesn't have unenforced checks.
    fn check_enforced_sql(&self) -> &'static str {
        "'YES'"
    }

    /// The `json_valid(...)` check that comes with a JSON column, named like the column.
    fn is_implicit_check(&self, name: &str, clause: &str) -> bool {
        clause.eq_ignore_ascii_case(&format!("json_valid(`{name}`)"))
    }

    fn normalize_column(&self, column: &mut ColumnDefinition) {
        let Some(raw) = column.default.take() else { return };
        // `DEFAULT NULL` and no default at all both come as the text NULL
        if raw.eq_ignore_ascii_case("null") {
            return;
        }
        if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
            column.default = Some(unquote(&raw[1..raw.len() - 1]));
        } else if raw.parse::<f64>().is_ok() || raw.starts_with("b'") {
            column.default = Some(raw);
        } else {
            // An expression: current_timestamp(), uuid(), (1 + 1)
            column.extra = format!("DEFAULT_GENERATED {}", column.extra).trim().to_string();
            column.default = Some(raw);
        }
    }

    /// A JSON column is a `longtext` the server guards with a `json_valid` check named like the
    /// column. It is told apart by that check, and read as the JSON it was declared as.
    async fn refine_columns(&self, conn: &mut mysql_async::Conn, database: &str, table: &str, columns: &mut Vec<StoredColumn>) -> mysql_async::Result<()> {
        if !columns.iter().any(|c| c.definition.data_type.eq_ignore_ascii_case("longtext")) {
            return Ok(());
        }
        let checks: Vec<(String, String)> = conn
            .exec(
                "SELECT CONSTRAINT_NAME, CHECK_CLAUSE FROM information_schema.CHECK_CONSTRAINTS \
                 WHERE CONSTRAINT_SCHEMA = ? AND TABLE_NAME = ? AND CHECK_CLAUSE LIKE 'json\\_valid(%'",
                (database, table),
            )
            .await?;
        for column in columns.iter_mut().filter(|c| c.definition.data_type.eq_ignore_ascii_case("longtext")) {
            let name = &column.definition.name;
            if checks.iter().any(|(constraint, clause)| constraint == name && self.is_implicit_check(constraint, clause)) {
                column.definition.data_type = "json".into();
                column.definition.column_type = "json".into();
            }
        }
        Ok(())
    }

    /// MariaDB has no `super_read_only`.
    fn read_only_sql(&self) -> &'static str {
        "SELECT @@read_only, 0"
    }

    /// `EXPLAIN FORMAT=JSON`, which doesn't run the statement, or `ANALYZE FORMAT=JSON`, which
    /// does; the plan is converted to the tree MySQL's comes as.
    async fn explain(&self, driver: &ServerDriver, statement: &str, analyze: bool) -> Result<ExplainResult, String> {
        let explain_sql = if analyze { format!("ANALYZE FORMAT=JSON {statement}") } else { format!("EXPLAIN FORMAT=JSON {statement}") };
        let endpoint = driver.endpoint().clone();
        let text = driver
            .editor()
            .run(move |conn| {
                Box::pin(async move { conn.query_first::<String, _>(explain_sql).await.map_err(|e| error_message(&e, &endpoint)) })
            })
            .await?
            .ok_or("The server returned no plan")?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("The server returned a plan DBunny can't read: {e}"))?;
        Ok(ExplainResult { plan: tree_from_json(&json)?, analyzed: analyze, format: ExplainFormat::MysqlTree })
    }

    async fn accounts(&self, conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>> {
        accounts::accounts(conn).await
    }

    async fn own_account(&self, conn: &mut mysql_async::Conn) -> mysql_async::Result<RoleInfo> {
        crate::db::mysql_common::admin::own_account(conn).await
    }

    async fn rich_sessions(&self, conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
        accounts::rich_sessions(conn, database).await
    }

    async fn blockers(&self, conn: &mut mysql_async::Conn) -> Vec<(i64, i64)> {
        accounts::blockers(conn).await
    }

    async fn session_ids_in(&self, conn: &mut mysql_async::Conn, name: &str) -> mysql_async::Result<Vec<i64>> {
        accounts::session_ids_in(conn, name).await
    }
}

/// The text of a quoted SQL string: `''` and a backslash escape stand for what they spell.
fn unquote(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\'', Some('\'')) => {
                chars.next();
                out.push('\'');
            }
            ('\\', Some(&next)) => {
                chars.next();
                out.push(match next {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => '\0',
                    other => other,
                });
            }
            (other, _) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_defaults_lose_their_quotes_and_escapes() {
        assert_eq!(unquote("x"), "x");
        assert_eq!(unquote("it''s"), "it's");
        assert_eq!(unquote("it\\'s \\\\ x"), "it's \\ x");
        assert_eq!(unquote("a\\nb"), "a\nb");
        assert_eq!(unquote(""), "");
    }

    #[test]
    fn mariadb_flavor_answers_as_mariadb() {
        let flavor = MariadbFlavor;
        assert_eq!(flavor.engine(), Engine::Mariadb);
        let v = ServerVersion { major: 10, minor: 6, patch: 0 };
        assert_eq!(flavor.capabilities(v), Capabilities::mariadb(v));
        assert_eq!(flavor.system_databases(), ["mysql", "sys", "information_schema", "performance_schema"]);
        assert_eq!(flavor.parse_version("11.4.13-MariaDB-ubu2404").unwrap().minor, 4);
    }
}
