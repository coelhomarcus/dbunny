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
}
