//! The serializable shapes the commands send and receive, shared by every driver.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::db::sql::filter::TableFilter;
use crate::db::ExplainFormat;

/// A row as column name -> value, for inserts and primary keys.
pub type JsonMap = serde_json::Map<String, JsonValue>;

/// What a session is connected to, as shown in the app.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub database: String,
    pub host: String,
    pub port: u16,
    pub server_version: String,
    /// The session can't write: read-only transactions or a read replica.
    pub read_only: bool,
}

// Catalog: explorer, columns, object definitions, autocomplete

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub schema: String,
    pub comment: Option<String>,
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

/// One object in the explorer.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerObject {
    pub name: String,
    /// table, view, matview, sequence, function, procedure or type.
    pub kind: String,
    /// Arguments of a function, a type's kind (enum, domain...), or "partitioned" and
    /// "foreign" for those tables.
    pub detail: String,
    /// A partitioned table's partitions; empty for everything else.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partitions: Vec<ExplorerPartition>,
}

/// One partition, which may itself be partitioned.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerPartition {
    pub schema: String,
    pub name: String,
    /// The values it holds, e.g. `FOR VALUES FROM ('2026-01-01') TO ('2027-01-01')`.
    pub bound: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partitions: Vec<ExplorerPartition>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerSchema {
    pub name: String,
    pub objects: Vec<ExplorerObject>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerTree {
    pub schemas: Vec<ExplorerSchema>,
    /// The first schema of the search path, opened first in the explorer.
    pub default_schema: Option<String>,
}

/// One column on a diagram card.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiagramColumn {
    pub name: String,
    pub type_name: String,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub ordinal: i32,
}

/// One table card. A table outside the opened schema is here only because a foreign key reaches it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiagramTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<DiagramColumn>,
}

/// One foreign key, including a key that covers several columns. `from` is the table that holds the key.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiagramRelation {
    pub name: String,
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

/// The tables and foreign keys of one schema, plus tables in other schemas those keys touch.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagram {
    pub tables: Vec<DiagramTable>,
    pub relations: Vec<DiagramRelation>,
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

// The Structure tab

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    /// "table", "view", "materialized view", "partitioned table" or "foreign table".
    pub kind: String,
    pub owner: String,
    pub comment: Option<String>,
    /// From the statistics; None when the table was never analyzed.
    pub row_estimate: Option<i64>,
    pub table_bytes: Option<i64>,
    pub indexes_bytes: Option<i64>,
    pub total_bytes: Option<i64>,
    /// The storage engine (MySQL); None for the engines that have only one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// The default collation of the table's text columns (MySQL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    /// Indexed columns or expressions, in order.
    pub columns: Vec<String>,
    pub method: String,
    pub is_unique: bool,
    pub is_primary: bool,
    /// Created by a PRIMARY KEY / UNIQUE / EXCLUDE constraint (so the constraint's DDL covers it).
    pub backs_constraint: bool,
    /// WHERE clause of a partial index.
    pub predicate: Option<String>,
    pub definition: String,
    pub bytes: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintInfo {
    pub name: String,
    /// "primary key", "unique", "check", "foreign key" or "exclusion".
    pub kind: String,
    pub definition: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

/// A foreign key in another table that points at this one.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedBy {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    /// BEFORE, AFTER or INSTEAD OF.
    pub timing: String,
    pub events: Vec<String>,
    /// ROW or STATEMENT.
    pub level: String,
    pub function: String,
    pub enabled: bool,
    pub definition: String,
}

/// What one role (or PUBLIC) may do on the table.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TablePrivilege {
    pub grantee: String,
    pub privileges: Vec<String>,
    /// Privileges it may grant on to others.
    pub grantable: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub meta: TableMeta,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub constraints: Vec<ConstraintInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub referenced_by: Vec<ReferencedBy>,
    pub triggers: Vec<TriggerInfo>,
    pub privileges: Vec<TablePrivilege>,
    pub ddl: String,
    /// Sections that couldn't be read (usually permissions), shown instead of failing the whole tab.
    pub warnings: Vec<String>,
}

// Table data and query results

/// What a read for the assistant may fetch.
#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    /// Rows to return; one more is read to know there were more.
    pub rows: usize,
    pub timeout: Duration,
}

impl ReadLimits {
    pub const DEFAULT: ReadLimits = ReadLimits { rows: 100, timeout: Duration::from_secs(30) };
}

/// The rows of a read for the assistant, at most `limits.rows`.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    /// The statement had more rows than `limits.rows`.
    pub more: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QueryColumn {
    pub name: String,
    pub data_type: String,
}

/// The most characters of one cell that a read for the assistant keeps; the agent is shown far
/// fewer, but a whole table of large documents must not be held to cut them afterwards.
pub const MAX_CELL_CHARS: usize = 100_000;

impl QueryColumn {
    /// `value` as a read keeps it: text past `MAX_CELL_CHARS` cut on a character boundary and binary
    /// past it described by its size (hex `\x…` in SQLite and MySQL, base64 in PostgreSQL).
    pub fn capped(&self, value: JsonValue) -> JsonValue {
        cap_cell(&self.data_type, value)
    }
}

/// `value` of a column of type `data_type`, as `QueryColumn::capped` keeps it.
pub fn cap_cell(data_type: &str, value: JsonValue) -> JsonValue {
    {
        let JsonValue::String(text) = &value else { return value };
        let Some((end, _)) = text.char_indices().nth(MAX_CELL_CHARS) else { return value };
        let kind = data_type.to_ascii_lowercase();
        if ["bytea", "blob", "binary"].iter().any(|word| kind.contains(word)) {
            let bytes = match text.strip_prefix("\\x") {
                Some(hex) => hex.len() / 2,
                None => text.len() / 4 * 3 - text.bytes().rev().take_while(|&b| b == b'=').count(),
            };
            return JsonValue::String(format!("<binary {bytes} bytes>"));
        }
        JsonValue::String(format!("{}…", &text[..end]))
    }
}

#[cfg(test)]
mod cell_tests {
    use super::*;

    fn column(data_type: &str) -> QueryColumn {
        QueryColumn { name: "c".into(), data_type: data_type.into() }
    }

    #[test]
    fn small_cells_stay_as_they_are() {
        assert_eq!(column("text").capped(JsonValue::from("abc")), JsonValue::from("abc"));
        assert_eq!(column("int4").capped(JsonValue::from(7)), JsonValue::from(7));
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let cut = column("text").capped(JsonValue::from("日".repeat(MAX_CELL_CHARS + 5)));
        let text = cut.as_str().unwrap();
        assert_eq!(text.chars().count(), MAX_CELL_CHARS + 1);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn long_binary_is_described_by_its_size() {
        let hex = format!("\\x{}", "ab".repeat(MAX_CELL_CHARS));
        assert_eq!(column("blob").capped(JsonValue::from(hex)), JsonValue::from(format!("<binary {} bytes>", MAX_CELL_CHARS)));
        let base64 = "AAAA".repeat(MAX_CELL_CHARS / 2) + "AA==";
        let size = MAX_CELL_CHARS / 2 * 3 + 1;
        assert_eq!(column("bytea").capped(JsonValue::from(base64)), JsonValue::from(format!("<binary {size} bytes>")));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
    pub rows_affected: Option<u64>,
    pub duration: u64,
    /// How many statements the script had (1 for a plain query).
    pub statement_count: usize,
    /// Notices the server sent while the script ran (`RAISE NOTICE`, warnings).
    pub notices: Vec<Notice>,
    /// The script ran inside the editor's open transaction.
    pub in_transaction: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResponse {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    pub total_rows: i64,
    /// True when `total_rows` comes from the table statistics instead of an exact count.
    pub total_rows_estimated: bool,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableDataParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub sort_column: Option<String>,
    pub sort_direction: Option<String>,
    /// Several sort keys, first one first; takes over from sort_column when given.
    pub sort: Option<Vec<SortKey>>,
    pub filters: Option<Vec<TableFilter>>,
    /// "any" joins the filters with OR; anything else (or none) with AND.
    pub filter_mode: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SortKey {
    pub column: String,
    /// "desc" for descending; anything else ascends.
    pub direction: Option<String>,
}

/// One edited row: its primary key and the new values.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    pub primary_key: serde_json::Map<String, JsonValue>,
    pub updates: serde_json::Map<String, JsonValue>,
}

// EXPLAIN

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    /// Postgres' JSON plan: `[{ "Plan": {...}, "Planning Time": .., "Execution Time": .. }]`,
    /// SQLite's query plan rows: `[{ "id": .., "parent": .., "detail": ".." }]`, or MySQL's plan
    /// tree: `[{ "label", "cost", "rows", "actualTimeFirst", "actualTimeLast", "actualRows",
    /// "loops", "children": [..] }]`.
    pub plan: JsonValue,
    /// The statement was actually run (ANALYZE), then rolled back.
    pub analyzed: bool,
    /// How `plan` is shaped.
    pub format: ExplainFormat,
}

// Structure changes

/// How a column's value is produced, instead of being stored as written.
/// `AUTOINCREMENT` is SQLite only; identity and `GENERATED` are checked per engine.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ColumnGeneration {
    #[default]
    None,
    IdentityByDefault,
    IdentityAlways,
    Autoincrement,
    Generated,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StructureChange {
    AddColumn {
        name: String,
        data_type: String,
        nullable: bool,
        default: Option<String>,
        comment: Option<String>,
        #[serde(default)]
        unique: bool,
        #[serde(default)]
        check: Option<String>,
        #[serde(default)]
        generation: ColumnGeneration,
        #[serde(default)]
        generated: Option<String>,
    },
    RenameColumn { name: String, new_name: String },
    DropColumn { name: String, cascade: bool },
    SetNullable { name: String, nullable: bool },
    SetDefault { name: String, default: Option<String> },
    SetComment { name: String, comment: Option<String> },
    /// `columns` are column names or expressions such as `lower(email)`.
    CreateIndex { name: Option<String>, columns: Vec<String>, unique: bool, method: String, concurrently: bool },
    DropIndex { name: String, cascade: bool },
    /// `using` converts the existing values when there's no automatic cast, e.g. `price::numeric`.
    SetType { name: String, data_type: String, using: Option<String> },
    AddForeignKey {
        name: Option<String>,
        columns: Vec<String>,
        ref_schema: String,
        ref_table: String,
        ref_columns: Vec<String>,
        on_update: String,
        on_delete: String,
    },
    AddUnique { name: Option<String>, columns: Vec<String> },
    AddCheck { name: Option<String>, expression: String },
    DropConstraint { name: String, cascade: bool },
    SetTriggerEnabled { name: String, enabled: bool },
    /// `grantee` is a role name, or PUBLIC for everyone.
    Grant { grantee: String, privileges: Vec<String>, grant_option: bool },
    Revoke { grantee: String, privileges: Vec<String> },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub default: Option<String>,
    /// A column-level `UNIQUE`. A primary key is unique on its own.
    #[serde(default)]
    pub unique: bool,
    /// A column-level `CHECK` expression, without a trailing semicolon.
    #[serde(default)]
    pub check: Option<String>,
    /// Shown after `CREATE TABLE` as `COMMENT ON COLUMN`. SQLite has no comments.
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub generation: ColumnGeneration,
    /// The expression for [`ColumnGeneration::Generated`], without a semicolon.
    #[serde(default)]
    pub generated: Option<String>,
}

/// A foreign key written inside `CREATE TABLE`.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewForeignKey {
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(default)]
    pub on_update: String,
    #[serde(default)]
    pub on_delete: String,
}

/// `PARTITION BY method (expression)` on a new PostgreSQL table. No child partitions.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewPartition {
    pub method: String,
    pub expression: String,
}

/// Everything the table builder sends. Omitted options stay off.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewTable {
    pub columns: Vec<NewColumn>,
    #[serde(default)]
    pub foreign_keys: Vec<NewForeignKey>,
    #[serde(default)]
    pub unlogged: bool,
    #[serde(default)]
    pub without_rowid: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub partition: Option<NewPartition>,
}

// Databases on the server

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub size_bytes: Option<i64>,
    pub is_current: bool,
    /// The user may connect to it.
    pub can_connect: bool,
}

// Roles

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleInfo {
    pub name: String,
    /// The host an account is for (MySQL names accounts `user@host`); None where roles have no host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub can_login: bool,
    pub superuser: bool,
    pub create_db: bool,
    pub create_role: bool,
    pub replication: bool,
    /// None where the engine has no row-level security to bypass.
    pub bypass_rls: Option<bool>,
    /// -1 when unlimited.
    pub connection_limit: i32,
    pub valid_until: Option<String>,
    /// Roles this one is a member of (inherits their rights unless NOINHERIT).
    pub member_of: Vec<String>,
    pub is_current: bool,
    /// Built in (pg_*), hidden unless asked for.
    pub is_builtin: bool,
}

/// The roles of a server. `limited_view` is set when the account may read only its own.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleList {
    pub roles: Vec<RoleInfo>,
    pub limited_view: bool,
}

// Server activity

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSession {
    /// The server's id of the session; MySQL's grow without end, so it is wide.
    pub pid: i64,
    pub user: Option<String>,
    pub database: Option<String>,
    pub application: Option<String>,
    pub client: Option<String>,
    /// active, idle, idle in transaction, ...
    pub state: Option<String>,
    /// What it waits for, e.g. "Lock: transactionid".
    pub waiting_for: Option<String>,
    /// Seconds since the current (or last) query started.
    pub query_seconds: Option<f64>,
    /// Seconds since the open transaction started.
    pub transaction_seconds: Option<f64>,
    pub query: Option<String>,
    /// Sessions holding locks this one waits on.
    pub blocked_by: Vec<i64>,
    /// The connection DBunny itself uses for this query.
    pub is_self: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerActivity {
    pub sessions: Vec<ServerSession>,
    /// The role may only see its own sessions' queries without pg_read_all_stats.
    pub limited_view: bool,
}

// Server messages

/// A notice, warning or other non-error message sent by the server.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
}
