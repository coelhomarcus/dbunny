//! What an engine supports, sent to the frontend on connect so it shows only the
//! screens and actions that work there.

use serde::Serialize;

use super::mysql_common::version::ServerVersion;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    // Optional pieces of the driver
    pub console: bool,
    pub transactions: bool,
    pub schema_editing: bool,
    pub admin: bool,
    /// The assistant can read through `Driver::reader`.
    pub assistant: bool,
    // Server screens, separate from `admin` so each engine picks the ones it has
    pub roles: bool,
    pub activity: bool,
    // Details
    pub schemas: bool,
    pub databases: bool,
    pub materialized_views: bool,
    pub partitions: bool,
    pub functions: bool,
    pub sequences: bool,
    pub types: bool,
    pub extensions: bool,
    pub comments: bool,
    pub alter_column: bool,
    pub constraints: bool,
    pub trigger_toggle: bool,
    pub permissions: bool,
    pub index_methods: bool,
    pub concurrent_index: bool,
    pub cascade: bool,
    pub truncate_options: bool,
    pub duplicate_table: bool,
    pub explain_analyze: bool,
    pub explain_format: ExplainFormat,
    pub returning: bool,
    pub ssh: bool,
    pub ssl: bool,
}

/// How the plan from `explain` is shaped, so the frontend knows how to draw it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ExplainFormat {
    /// `EXPLAIN (FORMAT JSON)`.
    PostgresJson,
    /// The rows of `EXPLAIN QUERY PLAN`.
    SqliteQueryPlan,
    /// `EXPLAIN FORMAT=TREE` (or `EXPLAIN ANALYZE`): indented text.
    MysqlTree,
}

impl Capabilities {
    /// PostgreSQL has every piece and every detail.
    pub fn postgres() -> Self {
        Self {
            console: true,
            transactions: true,
            schema_editing: true,
            admin: true,
            assistant: true,
            roles: true,
            activity: true,
            schemas: true,
            databases: true,
            materialized_views: true,
            partitions: true,
            functions: true,
            sequences: true,
            types: true,
            extensions: true,
            comments: true,
            alter_column: true,
            constraints: true,
            trigger_toggle: true,
            permissions: true,
            index_methods: true,
            concurrent_index: true,
            cascade: true,
            truncate_options: true,
            duplicate_table: true,
            explain_analyze: true,
            explain_format: ExplainFormat::PostgresJson,
            returning: true,
            ssh: true,
            ssl: true,
        }
    }

    /// SQLite is a file: it has the console, transactions and schema editing, but no
    /// server administration, schemas, databases or server-side objects.
    pub fn sqlite() -> Self {
        Self {
            console: true,
            transactions: true,
            schema_editing: true,
            admin: false,
            assistant: true,
            roles: false,
            activity: false,
            schemas: false,
            databases: false,
            materialized_views: false,
            partitions: false,
            functions: false,
            sequences: false,
            types: false,
            extensions: false,
            comments: false,
            alter_column: false,
            constraints: false,
            trigger_toggle: false,
            permissions: false,
            index_methods: false,
            concurrent_index: false,
            cascade: false,
            truncate_options: false,
            duplicate_table: true,
            explain_analyze: false,
            explain_format: ExplainFormat::SqliteQueryPlan,
            returning: true,
            ssh: false,
            ssl: false,
        }
    }

    /// MySQL 8.0+: the console, transactions, schema editing and administration of
    /// databases, users and sessions, but no schemas, sequences or RETURNING. What
    /// changes with the version is EXPLAIN ANALYZE (8.0.18).
    pub fn mysql(version: ServerVersion) -> Self {
        Self::mysql_family(version.at_least(8, 0, 18))
    }

    /// MariaDB 10.6+: the same pieces as MySQL, and `ANALYZE` in every supported version.
    pub fn mariadb(_version: ServerVersion) -> Self {
        Self::mysql_family(true)
    }

    fn mysql_family(explain_analyze: bool) -> Self {
        Self {
            console: true,
            transactions: true,
            schema_editing: true,
            admin: true,
            assistant: true,
            roles: true,
            activity: true,
            schemas: false,
            databases: true,
            materialized_views: false,
            partitions: false,
            functions: true,
            sequences: false,
            types: false,
            extensions: false,
            comments: true,
            alter_column: true,
            constraints: true,
            trigger_toggle: false,
            permissions: true,
            index_methods: true,
            concurrent_index: false,
            cascade: false,
            truncate_options: false,
            duplicate_table: true,
            explain_analyze,
            explain_format: ExplainFormat::MysqlTree,
            returning: false,
            ssh: true,
            ssl: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mysql_common::version::ServerVersion;

    #[test]
    fn capabilities_serialize_in_camel_case() {
        let json = serde_json::to_value(Capabilities::postgres()).unwrap();
        assert_eq!(json["materializedViews"], true);
        assert_eq!(json["explainFormat"], "postgresJson");
        assert_eq!(json["schemaEditing"], true);
        assert_eq!(json["assistant"], true);
    }

    #[test]
    fn postgres_has_every_piece() {
        let c = Capabilities::postgres();
        assert!(c.console && c.transactions && c.schema_editing && c.admin);
    }

    #[test]
    fn sqlite_has_no_server_pieces() {
        let c = Capabilities::sqlite();
        assert!(c.console && c.transactions && c.schema_editing && !c.admin && !c.schemas && !c.permissions);
        assert_eq!(c.explain_format, ExplainFormat::SqliteQueryPlan);
    }

    #[test]
    fn mysql_capabilities_follow_the_version() {
        let v = |major, minor, patch| ServerVersion { major, minor, patch };
        let c = Capabilities::mysql(v(8, 4, 0));
        assert!(c.console && c.transactions && c.schema_editing && c.admin && c.databases && c.roles && c.activity);
        assert!(!c.schemas && !c.returning && !c.trigger_toggle && !c.cascade && !c.sequences);
        assert!(c.explain_analyze && c.constraints);
        assert_eq!(c.explain_format, ExplainFormat::MysqlTree);
        assert!(!Capabilities::mysql(v(8, 0, 17)).explain_analyze);
        assert!(Capabilities::mysql(v(8, 0, 18)).explain_analyze);
        assert_eq!(serde_json::to_value(&c).unwrap()["explainFormat"], "mysqlTree");
    }

    #[test]
    fn mariadb_capabilities() {
        let v = |major, minor, patch| ServerVersion { major, minor, patch };
        let c = Capabilities::mariadb(v(10, 6, 0));
        assert!(c.console && c.transactions && c.schema_editing && c.admin && c.databases && c.roles && c.activity);
        assert!(!c.schemas && !c.returning && !c.trigger_toggle && !c.cascade && !c.sequences);
        assert!(c.explain_analyze && c.constraints);
        assert_eq!(c.explain_format, ExplainFormat::MysqlTree);
        // The same pieces as MySQL, whatever the version
        assert_eq!(Capabilities::mariadb(v(11, 4, 0)), Capabilities { explain_analyze: true, ..Capabilities::mysql(v(8, 4, 0)) });
    }

    #[test]
    fn postgres_keeps_roles_and_activity_and_sqlite_has_neither() {
        assert!(Capabilities::postgres().roles && Capabilities::postgres().activity);
        assert!(!Capabilities::sqlite().roles && !Capabilities::sqlite().activity);
    }

    #[test]
    fn sqlite_capabilities_serialize_in_camel_case() {
        let json = serde_json::to_value(Capabilities::sqlite()).unwrap();
        assert_eq!(json["explainFormat"], "sqliteQueryPlan");
        assert_eq!(json["duplicateTable"], true);
        assert_eq!(json["returning"], true);
        assert_eq!(json["databases"], false);
    }
}
