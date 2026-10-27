//! The driver contract. Every engine implements the core (`Driver`, `Catalog`, `Data`);
//! the other pieces are optional and a driver hands them out only when its engine has them.

use std::sync::Arc;

use async_trait::async_trait;

use super::capabilities::Capabilities;
use super::model::{
    ColumnInfo, CompletionSchema, ConnectionInfo, DatabaseInfo, Diagram, ExplainResult, ReadLimits, ReadResult, ExplorerTree, ExtensionInfo,
    FunctionDetail, JsonMap, NewTable, ObjectDefinition, QueryResult, RoleList, RowChange, ServerActivity,
    StructureChange, TableDataParams, TableDataResponse, TableStructure,
};
use super::version::ServerVersion;
use super::Engine;

/// One open session on a database.
#[async_trait]
pub trait Driver: Send + Sync {
    fn engine(&self) -> Engine;
    fn capabilities(&self) -> &Capabilities;
    /// Database, host, port, server version and whether the session is read-only.
    fn info(&self) -> ConnectionInfo;
    /// The server's version, when the engine has one DBunny reads (not SQLite).
    fn version(&self) -> Option<ServerVersion> {
        None
    }
    async fn close(&self);
    fn catalog(&self) -> &dyn Catalog;
    fn data(&self) -> &dyn Data;
    fn console(&self) -> Option<&dyn Console> {
        None
    }
    fn transactions(&self) -> Option<&dyn Transactions> {
        None
    }
    fn schema_editor(&self) -> Option<&dyn SchemaEditor> {
        None
    }
    fn admin(&self) -> Option<&dyn ServerAdmin> {
        None
    }
    /// Reading for the assistant, apart from the editor's connection and its transaction.
    fn reader(&self) -> Option<&dyn ReadOnlyQueries> {
        None
    }
}

/// Reading what the database holds: the explorer, columns, structure, autocomplete.
#[async_trait]
pub trait Catalog: Send + Sync {
    async fn explorer(&self) -> Result<ExplorerTree, String>;
    /// Tables, columns and foreign keys for the diagram of `schema`, including tables
    /// outside it that a key of this schema touches.
    async fn diagram(&self, schema: &str) -> Result<Diagram, String>;
    async fn columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnInfo>, String>;
    async fn relation_kind(&self, schema: &str, name: &str) -> Result<Option<String>, String>;
    async fn table_structure(&self, schema: &str, table: &str) -> Result<TableStructure, String>;
    async fn completion_schema(&self) -> Result<CompletionSchema, String>;
    async fn object_definition(&self, schema: &str, name: &str, kind: &str) -> Result<ObjectDefinition, String>;
    /// Engines without functions answer `unsupported(engine, "Functions")`.
    async fn function_detail(&self, schema: &str, name: &str, args: &str) -> Result<FunctionDetail, String>;
    /// Engines without extensions answer `unsupported(engine, "Extensions")`.
    async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String>;
}

/// A table's rows: paging, counting and editing.
#[async_trait]
pub trait Data: Send + Sync {
    async fn table_page(&self, schema: &str, table: &str, params: TableDataParams) -> Result<TableDataResponse, String>;
    async fn count_rows(&self, schema: &str, table: &str) -> Result<i64, String>;
    async fn insert_row(&self, schema: &str, table: &str, values: JsonMap) -> Result<u64, String>;
    /// Every row is saved or none is.
    async fn save_rows(&self, schema: &str, table: &str, changes: Vec<RowChange>) -> Result<u64, String>;
    async fn delete_rows(&self, schema: &str, table: &str, keys: Vec<JsonMap>) -> Result<u64, String>;
    async fn import_rows(
        &self,
        schema: &str,
        table: &str,
        columns: Vec<String>,
        rows: Vec<Vec<Option<String>>>,
    ) -> Result<u64, String>;
}

/// The SQL editor.
#[async_trait]
pub trait Console: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<QueryResult, String>;
    async fn cancel(&self) -> Result<(), String>;
    async fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainResult, String>;
}

/// What the assistant may run: one statement that only reads, on a connection of its own.
#[async_trait]
pub trait ReadOnlyQueries: Send + Sync {
    /// Runs one statement that only reads, outside the editor's connection, in a read-only
    /// transaction that is always rolled back. Reads at most `limits.rows + 1` rows and stops.
    async fn query(&self, sql: &str, limits: ReadLimits) -> Result<ReadResult, String>;

    /// The plan of one statement that only reads, run for real (EXPLAIN ANALYZE) under the same
    /// terms as `query`: a read-only transaction on a connection apart from the editor's.
    async fn explain_analyzed(&self, sql: &str, limits: ReadLimits) -> Result<ExplainResult, String>;
}

/// A transaction kept open across runs of the SQL editor.
#[async_trait]
pub trait Transactions: Send + Sync {
    async fn begin(&self) -> Result<(), String>;
    async fn commit(&self) -> Result<(), String>;
    async fn rollback(&self) -> Result<(), String>;
    async fn is_open(&self) -> Result<bool, String>;
}

/// Changing tables and other objects.
#[async_trait]
pub trait SchemaEditor: Send + Sync {
    async fn preview_change(&self, schema: &str, table: &str, change: &StructureChange) -> Result<String, String>;
    async fn apply_change(&self, schema: &str, table: &str, change: StructureChange) -> Result<(), String>;
    async fn preview_create_table(&self, schema: &str, table: &str, spec: &NewTable) -> Result<String, String>;
    async fn create_table(&self, schema: &str, table: &str, spec: NewTable) -> Result<(), String>;
    async fn rename_relation(&self, schema: &str, name: &str, kind: &str, new_name: &str) -> Result<(), String>;
    async fn drop_relation(&self, schema: &str, name: &str, kind: &str, cascade: bool) -> Result<(), String>;
    async fn truncate_table(&self, schema: &str, name: &str, restart_identity: bool, cascade: bool) -> Result<(), String>;
    async fn duplicate_table(&self, schema: &str, name: &str, new_name: &str, with_data: bool) -> Result<(), String>;
    async fn refresh_materialized_view(&self, schema: &str, name: &str) -> Result<(), String>;
}

/// The server around the database: other databases, schemas, roles and activity.
#[async_trait]
pub trait ServerAdmin: Send + Sync {
    async fn databases(&self) -> Result<Vec<DatabaseInfo>, String>;
    async fn open_database(&self, name: &str) -> Result<Arc<dyn Driver>, String>;
    async fn create_database(&self, name: &str) -> Result<(), String>;
    async fn drop_database(&self, name: &str, force: bool) -> Result<(), String>;
    async fn create_schema(&self, name: &str) -> Result<(), String>;
    async fn rename_schema(&self, name: &str, new_name: &str) -> Result<(), String>;
    async fn drop_schema(&self, name: &str, cascade: bool) -> Result<(), String>;
    async fn roles(&self) -> Result<RoleList, String>;
    async fn activity(&self, all_databases: bool) -> Result<ServerActivity, String>;
    async fn signal_backend(&self, pid: i64, terminate: bool) -> Result<bool, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the core: no console, transactions, schema editing or administration.
    struct FakeDriver {
        capabilities: Capabilities,
    }

    impl Default for FakeDriver {
        fn default() -> Self {
            Self {
                capabilities: Capabilities {
                    console: false,
                    transactions: false,
                    schema_editing: false,
                    admin: false,
                    ..Capabilities::postgres(ServerVersion { major: 18, minor: 0, patch: 0 })
                },
            }
        }
    }

    #[async_trait]
    impl Driver for FakeDriver {
        fn engine(&self) -> Engine {
            Engine::Postgres
        }
        fn capabilities(&self) -> &Capabilities {
            &self.capabilities
        }
        fn info(&self) -> ConnectionInfo {
            unimplemented!()
        }
        async fn close(&self) {}
        fn catalog(&self) -> &dyn Catalog {
            self
        }
        fn data(&self) -> &dyn Data {
            self
        }
    }

    #[async_trait]
    impl Catalog for FakeDriver {
        async fn explorer(&self) -> Result<ExplorerTree, String> {
            unimplemented!()
        }
        async fn diagram(&self, _: &str) -> Result<Diagram, String> {
            unimplemented!()
        }
        async fn columns(&self, _: &str, _: &str) -> Result<Vec<ColumnInfo>, String> {
            unimplemented!()
        }
        async fn relation_kind(&self, _: &str, _: &str) -> Result<Option<String>, String> {
            unimplemented!()
        }
        async fn table_structure(&self, _: &str, _: &str) -> Result<TableStructure, String> {
            unimplemented!()
        }
        async fn completion_schema(&self) -> Result<CompletionSchema, String> {
            unimplemented!()
        }
        async fn object_definition(&self, _: &str, _: &str, _: &str) -> Result<ObjectDefinition, String> {
            unimplemented!()
        }
        async fn function_detail(&self, _: &str, _: &str, _: &str) -> Result<FunctionDetail, String> {
            unimplemented!()
        }
        async fn extensions(&self) -> Result<Vec<ExtensionInfo>, String> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl Data for FakeDriver {
        async fn table_page(&self, _: &str, _: &str, _: TableDataParams) -> Result<TableDataResponse, String> {
            unimplemented!()
        }
        async fn count_rows(&self, _: &str, _: &str) -> Result<i64, String> {
            unimplemented!()
        }
        async fn insert_row(&self, _: &str, _: &str, _: JsonMap) -> Result<u64, String> {
            unimplemented!()
        }
        async fn save_rows(&self, _: &str, _: &str, _: Vec<RowChange>) -> Result<u64, String> {
            unimplemented!()
        }
        async fn delete_rows(&self, _: &str, _: &str, _: Vec<JsonMap>) -> Result<u64, String> {
            unimplemented!()
        }
        async fn import_rows(&self, _: &str, _: &str, _: Vec<String>, _: Vec<Vec<Option<String>>>) -> Result<u64, String> {
            unimplemented!()
        }
    }

    /// An engine that has to read its catalog (await) before it can tell what a change would run.
    struct AwaitingEditor;

    #[async_trait]
    impl SchemaEditor for AwaitingEditor {
        async fn preview_change(&self, schema: &str, table: &str, _: &StructureChange) -> Result<String, String> {
            tokio::task::yield_now().await;
            Ok(format!("-- change {schema}.{table}"))
        }
        async fn apply_change(&self, _: &str, _: &str, _: StructureChange) -> Result<(), String> {
            unimplemented!()
        }
        async fn preview_create_table(&self, schema: &str, table: &str, _: &NewTable) -> Result<String, String> {
            tokio::task::yield_now().await;
            Ok(format!("-- create {schema}.{table}"))
        }
        async fn create_table(&self, _: &str, _: &str, _: NewTable) -> Result<(), String> {
            unimplemented!()
        }
        async fn rename_relation(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), String> {
            unimplemented!()
        }
        async fn drop_relation(&self, _: &str, _: &str, _: &str, _: bool) -> Result<(), String> {
            unimplemented!()
        }
        async fn truncate_table(&self, _: &str, _: &str, _: bool, _: bool) -> Result<(), String> {
            unimplemented!()
        }
        async fn duplicate_table(&self, _: &str, _: &str, _: &str, _: bool) -> Result<(), String> {
            unimplemented!()
        }
        async fn refresh_materialized_view(&self, _: &str, _: &str) -> Result<(), String> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn previews_can_await() {
        let editor: &dyn SchemaEditor = &AwaitingEditor;
        let change = StructureChange::DropColumn { name: "a".into(), cascade: false };
        assert_eq!(editor.preview_change("s", "t", &change).await.unwrap(), "-- change s.t");
        assert_eq!(editor.preview_create_table("s", "t", &NewTable::default()).await.unwrap(), "-- create s.t");
    }

    #[tokio::test]
    async fn missing_pieces_report_unsupported() {
        let driver: Arc<dyn Driver> = Arc::new(FakeDriver::default());
        assert!(driver.console().is_none() && driver.admin().is_none() && driver.reader().is_none());
        let err = crate::commands::require(driver.admin(), driver.engine(), "Roles").err().unwrap();
        assert_eq!(err, "Roles isn't supported by PostgreSQL");
    }
}
