//! A driver for the tests of the assistant: a small catalog, and a record of what it was asked to read.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db::model::*;
use crate::db::{Capabilities, Catalog, Console, Data, Driver, Engine, ExplainFormat, ReadOnlyQueries};

pub struct FakeDb {
    pub read_sql: Mutex<Vec<String>>,
    pub explained: Mutex<Vec<(String, bool)>>,
    /// The statements explained with ANALYZE through the reader
    pub analyzed: Mutex<Vec<String>>,
    pub rows: Vec<Vec<Value>>,
    capabilities: Capabilities,
}

impl FakeDb {
    pub fn new() -> Arc<FakeDb> {
        Arc::new(FakeDb {
            read_sql: Default::default(),
            explained: Default::default(),
            analyzed: Default::default(),
            rows: vec![vec![json!(1), json!("ana")], vec![json!(2), json!("bo")]],
            capabilities: Capabilities::postgres(crate::db::version::ServerVersion { major: 18, minor: 0, patch: 0 }),
        })
    }

    pub fn reads(&self) -> Vec<String> {
        self.read_sql.lock().unwrap().clone()
    }
}

#[async_trait]
impl Driver for FakeDb {
    fn engine(&self) -> Engine {
        Engine::Postgres
    }
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    fn info(&self) -> ConnectionInfo {
        ConnectionInfo { database: "shop".into(), host: "localhost".into(), port: 5432, server_version: "16.2".into(), read_only: false }
    }
    async fn close(&self) {}
    fn catalog(&self) -> &dyn Catalog {
        self
    }
    fn data(&self) -> &dyn Data {
        self
    }
    fn console(&self) -> Option<&dyn Console> {
        Some(self)
    }
    fn reader(&self) -> Option<&dyn ReadOnlyQueries> {
        Some(self)
    }
}

#[async_trait]
impl Catalog for FakeDb {
    async fn explorer(&self) -> Result<ExplorerTree, String> {
        let object = |name: &str, kind: &str| ExplorerObject { name: name.into(), kind: kind.into(), detail: String::new(), partitions: vec![] };
        Ok(ExplorerTree {
            schemas: vec![
                ExplorerSchema { name: "public".into(), objects: vec![object("orders", "table"), object("recent", "view"), object("total", "function")] },
                ExplorerSchema { name: "audit".into(), objects: vec![object("log", "table")] },
            ],
            default_schema: Some("public".into()),
        })
    }
    async fn diagram(&self, schema: &str) -> Result<Diagram, String> {
        Ok(Diagram {
            tables: vec![],
            relations: vec![DiagramRelation {
                name: "orders_user_fk".into(),
                from_schema: schema.into(),
                from_table: "orders".into(),
                from_columns: vec!["user_id".into()],
                to_schema: schema.into(),
                to_table: "users".into(),
                to_columns: vec!["id".into()],
            }],
        })
    }
    async fn columns(&self, _: &str, _: &str) -> Result<Vec<ColumnInfo>, String> {
        unimplemented!()
    }
    async fn relation_kind(&self, _: &str, _: &str) -> Result<Option<String>, String> {
        unimplemented!()
    }
    async fn table_structure(&self, schema: &str, table: &str) -> Result<TableStructure, String> {
        let column = |name: &str, data_type: &str, primary: bool| ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            type_name: data_type.into(),
            is_nullable: !primary,
            default_value: None,
            ordinal_position: 1,
            is_primary_key: primary,
            is_generated: false,
            comment: None,
            enum_values: None,
            foreign_key: None,
        };
        let _ = (schema, table);
        Ok(TableStructure {
            meta: TableMeta { kind: "table".into(), comment: Some("Customer orders".into()), ..Default::default() },
            columns: vec![column("id", "int4", true), column("user_id", "int4", false)],
            indexes: vec![],
            constraints: vec![],
            foreign_keys: vec![ForeignKeyInfo {
                name: "orders_user_fk".into(),
                columns: vec!["user_id".into()],
                ref_schema: "public".into(),
                ref_table: "users".into(),
                ref_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "NO ACTION".into(),
            }],
            referenced_by: vec![],
            triggers: vec![],
            privileges: vec![],
            ddl: "CREATE TABLE ...".into(),
            warnings: vec![],
        })
    }
    async fn completion_schema(&self) -> Result<CompletionSchema, String> {
        Ok(CompletionSchema {
            current_schema: Some("public".into()),
            relations: vec![CompletionRelation {
                schema: "public".into(),
                name: "orders".into(),
                relation_type: "table".into(),
                columns: vec![CompletionColumn { name: "id".into(), data_type: "int4".into(), ..Default::default() }],
            }],
            functions: vec![],
            builtin_functions: vec![],
        })
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
impl Data for FakeDb {
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

#[async_trait]
impl Console for FakeDb {
    async fn execute(&self, _: &str) -> Result<QueryResult, String> {
        unimplemented!()
    }
    async fn cancel(&self) -> Result<(), String> {
        unimplemented!()
    }
    async fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainResult, String> {
        self.explained.lock().unwrap().push((sql.to_string(), analyze));
        Ok(ExplainResult { plan: json!([{"Plan": {"Node Type": "Seq Scan"}}]), analyzed: analyze, format: ExplainFormat::PostgresJson })
    }
}

#[async_trait]
impl ReadOnlyQueries for FakeDb {
    async fn query(&self, sql: &str, _limits: ReadLimits) -> Result<ReadResult, String> {
        self.read_sql.lock().unwrap().push(sql.to_string());
        Ok(ReadResult {
            columns: vec![QueryColumn { name: "id".into(), data_type: "int4".into() }, QueryColumn { name: "name".into(), data_type: "text".into() }],
            rows: self.rows.clone(),
            more: false,
        })
    }

    async fn explain_analyzed(&self, sql: &str, _limits: ReadLimits) -> Result<ExplainResult, String> {
        self.analyzed.lock().unwrap().push(sql.to_string());
        Ok(ExplainResult { plan: json!([{"Plan": {"Node Type": "Seq Scan"}}]), analyzed: true, format: ExplainFormat::PostgresJson })
    }
}
