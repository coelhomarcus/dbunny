//! What an engine supports, sent to the frontend on connect so it shows only the
//! screens and actions that work there.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    // Optional pieces of the driver
    pub console: bool,
    pub transactions: bool,
    pub schema_editing: bool,
    pub admin: bool,
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
}

impl Capabilities {
    /// PostgreSQL has every piece and every detail.
    pub fn postgres() -> Self {
        Self {
            console: true,
            transactions: true,
            schema_editing: true,
            admin: true,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_serialize_in_camel_case() {
        let json = serde_json::to_value(Capabilities::postgres()).unwrap();
        assert_eq!(json["materializedViews"], true);
        assert_eq!(json["explainFormat"], "postgresJson");
        assert_eq!(json["schemaEditing"], true);
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
    fn sqlite_capabilities_serialize_in_camel_case() {
        let json = serde_json::to_value(Capabilities::sqlite()).unwrap();
        assert_eq!(json["explainFormat"], "sqliteQueryPlan");
        assert_eq!(json["duplicateTable"], true);
        assert_eq!(json["returning"], true);
        assert_eq!(json["databases"], false);
    }
}
