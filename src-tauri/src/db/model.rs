//! The serializable shapes the commands send and receive, shared by every driver.

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryColumn {
    pub name: String,
    pub data_type: String,
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
    /// or SQLite's query plan rows: `[{ "id": .., "parent": .., "detail": ".." }]`.
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
    pub can_login: bool,
    pub superuser: bool,
    pub create_db: bool,
    pub create_role: bool,
    pub replication: bool,
    pub bypass_rls: bool,
    /// -1 when unlimited.
    pub connection_limit: i32,
    pub valid_until: Option<String>,
    /// Roles this one is a member of (inherits their rights unless NOINHERIT).
    pub member_of: Vec<String>,
    pub is_current: bool,
    /// Built in (pg_*), hidden unless asked for.
    pub is_builtin: bool,
}

// Server activity

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSession {
    pub pid: i32,
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
    pub blocked_by: Vec<i32>,
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
