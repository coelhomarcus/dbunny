//! The tools the agent works with: what the database holds, and queries that only read.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::approvals::{ApprovalRequest, Approvals, Decision};
use super::conversation::Conversation;
use super::events::EventSink;
use super::result::{agent_result, MAX_RESULT_BYTES};
use crate::db::model::ReadLimits;
use crate::db::sql::reading::check_reading;
use crate::db::sql::Dialect;
use crate::db::{Driver, Engine};

pub const DECLINED: &str = "The user declined to run this query. Ask them how to continue.";
pub const SCHEMA_ONLY: &str = "This connection doesn't share values with the assistant. Only column names and the row count are returned.";
pub const CLOSED: &str = "The connection was closed.";

pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    fn ok(value: Value) -> ToolOutput {
        ToolOutput { text: value.to_string(), is_error: false }
    }

    fn error(text: impl Into<String>) -> ToolOutput {
        ToolOutput { text: text.into(), is_error: true }
    }
}

pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn definitions() -> Vec<ToolDefinition> {
    let text = |description: &str| json!({"type": "string", "description": description});
    let object = |properties: Value, required: Vec<&str>| json!({"type": "object", "properties": properties, "required": required});
    vec![
        ToolDefinition {
            name: "list_tables",
            description: "List the tables and views of the database, by schema. On MySQL and MariaDB a schema is a database; on SQLite it is `main`. Takes no approval.",
            input_schema: object(json!({"schema": text("Only this schema; omit for all")}), vec![]),
        },
        ToolDefinition {
            name: "describe_table",
            description: "Describe a table or view: columns with types, nullability and defaults, primary key, indexes, constraints and foreign keys in both directions. Use it before writing SQL against a table.",
            input_schema: object(json!({"schema": text("The schema of the table"), "table": text("The table or view name")}), vec!["schema", "table"]),
        },
        ToolDefinition {
            name: "relationships",
            description: "List the foreign keys of a schema as `schema.table(columns) -> schema.table(columns)`, to know how tables join.",
            input_schema: object(json!({"schema": text("The schema")}), vec!["schema"]),
        },
        ToolDefinition {
            name: "run_query",
            description: "Run one SQL statement that only reads (SELECT, WITH ... SELECT, and SHOW or DESCRIBE where the engine has them) and get its rows. The person approves each query first. Rows are limited; add a LIMIT and filters. Statements that write are refused: show them to the person in a sql block instead.",
            input_schema: object(json!({"sql": text("The statement to run")}), vec!["sql"]),
        },
        ToolDefinition {
            name: "explain_query",
            description: "Get the execution plan of one statement. Without analyze nothing is executed. With analyze the statement really runs (it must only read) and the person approves it first.",
            input_schema: object(json!({"sql": text("The statement to explain"), "analyze": {"type": "boolean", "description": "Run it and report real times; default false"}}), vec!["sql"]),
        },
    ]
}

fn dialect_of(engine: Engine) -> Dialect {
    match engine {
        Engine::Postgres => Dialect::Postgres,
        Engine::Sqlite => Dialect::Sqlite,
        Engine::Mysql | Engine::Mariadb => Dialect::Mysql,
    }
}

pub struct Tools {
    /// The open database sessions, by id (the app's own map).
    pub sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>>,
    pub approvals: Arc<Approvals>,
    pub events: Arc<dyn EventSink>,
}

impl Tools {
    pub async fn call(&self, conversation: &Conversation, name: &str, args: Value) -> ToolOutput {
        let Some(driver) = self.sessions.lock().await.get(&conversation.driver_session).cloned() else {
            return ToolOutput::error(CLOSED);
        };
        let argument = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_string);
        let need = |key: &str| argument(key).ok_or_else(|| ToolOutput::error(format!("Missing argument: {key}")));
        let outcome = match name {
            "list_tables" => self.list_tables(&driver, argument("schema")).await,
            "describe_table" => match (need("schema"), need("table")) {
                (Ok(schema), Ok(table)) => describe_table(&driver, &schema, &table).await,
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            "relationships" => match need("schema") {
                Ok(schema) => relationships(&driver, &schema).await,
                Err(e) => Err(e),
            },
            "run_query" => match need("sql") {
                Ok(sql) => self.run_query(conversation, &driver, &sql).await,
                Err(e) => Err(e),
            },
            "explain_query" => match need("sql") {
                Ok(sql) => {
                    let analyze = args.get("analyze").and_then(Value::as_bool).unwrap_or(false);
                    self.explain_query(conversation, &driver, &sql, analyze).await
                }
                Err(e) => Err(e),
            },
            other => Err(ToolOutput::error(format!("There is no tool called {other}."))),
        };
        outcome.unwrap_or_else(|failure| failure)
    }

    async fn list_tables(&self, driver: &Arc<dyn Driver>, only: Option<String>) -> Result<ToolOutput, ToolOutput> {
        let tree = driver.catalog().explorer().await.map_err(ToolOutput::error)?;
        let schemas: Vec<Value> = tree
            .schemas
            .iter()
            .filter(|schema| only.as_deref().is_none_or(|name| name == schema.name))
            .filter_map(|schema| {
                let tables: Vec<Value> = schema
                    .objects
                    .iter()
                    .filter(|object| matches!(object.kind.as_str(), "table" | "view" | "matview"))
                    .map(|object| json!({"name": object.name, "kind": object.kind}))
                    .collect();
                (!tables.is_empty()).then(|| json!({"name": schema.name, "tables": tables}))
            })
            .collect();
        Ok(ToolOutput::ok(json!({"schemas": schemas})))
    }

    /// Asks the person unless they allowed reads for this conversation; None means go ahead.
    async fn approve(&self, conversation: &Conversation, tool: &str, sql: &str, analyze: bool) -> Option<ToolOutput> {
        if !conversation.options.ask_before_queries || conversation.reads_allowed.load(Ordering::SeqCst) {
            return None;
        }
        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.id.clone(),
            tool: tool.to_string(),
            sql: sql.to_string(),
            analyze,
        };
        match self.approvals.ask(request, self.events.as_ref()).await {
            Decision::Once => None,
            Decision::ReadsInChat => {
                conversation.reads_allowed.store(true, Ordering::SeqCst);
                None
            }
            Decision::Decline => Some(ToolOutput::error(DECLINED)),
        }
    }

    async fn run_query(&self, conversation: &Conversation, driver: &Arc<dyn Driver>, sql: &str) -> Result<ToolOutput, ToolOutput> {
        check_reading(dialect_of(driver.engine()), sql).map_err(|refusal| ToolOutput::error(refusal.message()))?;
        let reader = driver.reader().ok_or_else(|| ToolOutput::error("This database can't be read by the assistant."))?;
        if let Some(refused) = self.approve(conversation, "run_query", sql, false).await {
            return Err(refused);
        }
        let limits = ReadLimits { rows: conversation.options.row_limit, ..ReadLimits::DEFAULT };
        let result = reader.query(sql, limits).await.map_err(ToolOutput::error)?;
        Ok(ToolOutput { text: agent_result(&result, limits.rows, conversation.options.schema_only), is_error: false })
    }

    async fn explain_query(&self, conversation: &Conversation, driver: &Arc<dyn Driver>, sql: &str, analyze: bool) -> Result<ToolOutput, ToolOutput> {
        let console = driver.console().ok_or_else(|| ToolOutput::error("This database can't explain queries."))?;
        if analyze {
            check_reading(dialect_of(driver.engine()), sql).map_err(|refusal| ToolOutput::error(refusal.message()))?;
            if conversation.options.schema_only {
                return Err(ToolOutput::error(SCHEMA_ONLY));
            }
            if let Some(refused) = self.approve(conversation, "explain_query", sql, true).await {
                return Err(refused);
            }
        }
        // A plan that runs the statement goes where the reads go; without ANALYZE nothing runs.
        // A server that can't analyze answers through the console, with its own reason.
        let plan = match driver.reader() {
            Some(reader) if analyze && driver.capabilities().explain_analyze => reader.explain_analyzed(sql, ReadLimits::DEFAULT).await,
            _ => console.explain(sql, analyze).await,
        }
        .map_err(ToolOutput::error)?;
        let text = serde_json::to_string(&plan).map_err(|e| ToolOutput::error(e.to_string()))?;
        if text.len() <= MAX_RESULT_BYTES {
            return Ok(ToolOutput { text, is_error: false });
        }
        let cut = text.char_indices().map(|(i, _)| i).take_while(|&i| i <= MAX_RESULT_BYTES - 200).last().unwrap_or(0);
        Ok(ToolOutput::ok(json!({"truncated": true, "plan": &text[..cut]})))
    }
}

async fn describe_table(driver: &Arc<dyn Driver>, schema: &str, table: &str) -> Result<ToolOutput, ToolOutput> {
    let structure = driver.catalog().table_structure(schema, table).await.map_err(ToolOutput::error)?;
    let columns: Vec<Value> = structure
        .columns
        .iter()
        .map(|c| {
            json!({
                "name": c.name, "type": c.type_name, "nullable": c.is_nullable, "default": c.default_value,
                "primaryKey": c.is_primary_key, "generated": c.is_generated, "comment": c.comment, "enumValues": c.enum_values,
            })
        })
        .collect();
    Ok(ToolOutput::ok(json!({
        "schema": schema,
        "table": table,
        "kind": structure.meta.kind,
        "comment": structure.meta.comment,
        "columns": columns,
        "indexes": structure.indexes.iter().map(|i| json!({"name": i.name, "columns": i.columns, "unique": i.is_unique, "primary": i.is_primary})).collect::<Vec<_>>(),
        "constraints": structure.constraints.iter().map(|c| json!({"name": c.name, "kind": c.kind, "definition": c.definition})).collect::<Vec<_>>(),
        "foreignKeys": structure.foreign_keys.iter().map(|f| json!({"name": f.name, "columns": f.columns, "refSchema": f.ref_schema, "refTable": f.ref_table, "refColumns": f.ref_columns})).collect::<Vec<_>>(),
        "referencedBy": structure.referenced_by.iter().map(|r| json!({"name": r.name, "schema": r.schema, "table": r.table, "columns": r.columns})).collect::<Vec<_>>(),
    })))
}

async fn relationships(driver: &Arc<dyn Driver>, schema: &str) -> Result<ToolOutput, ToolOutput> {
    let diagram = driver.catalog().diagram(schema).await.map_err(ToolOutput::error)?;
    let relations: Vec<String> = diagram
        .relations
        .iter()
        .map(|r| {
            format!(
                "{}.{}({}) -> {}.{}({})",
                r.from_schema,
                r.from_table,
                r.from_columns.join(", "),
                r.to_schema,
                r.to_table,
                r.to_columns.join(", ")
            )
        })
        .collect();
    Ok(ToolOutput::ok(json!({"relations": relations})))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::assistant::conversation::{ConversationOptions, Conversations};
    use crate::assistant::events::MemorySink;
    use crate::assistant::fake::FakeDb;
    use crate::db::sql::reading::READ_REFUSED;

    struct Fixture {
        tools: Arc<Tools>,
        conversation: Arc<Conversation>,
        db: Arc<FakeDb>,
        sink: Arc<MemorySink>,
    }

    fn options(ask: bool, schema_only: bool) -> ConversationOptions {
        ConversationOptions { ask_before_queries: ask, schema_only, row_limit: 100, instructions: String::new() }
    }

    async fn fixture(options: ConversationOptions) -> Fixture {
        let db = FakeDb::new();
        let (sink, approvals) = (MemorySink::new(), Arc::new(Approvals::default()));
        let sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>> = Default::default();
        sessions.lock().await.insert("s1".into(), db.clone());
        let conversation = Conversations::default().create("s1", options);
        let events: Arc<dyn EventSink> = sink.clone();
        Fixture { tools: Arc::new(Tools { sessions, approvals, events }), conversation, db, sink }
    }

    impl Fixture {
        async fn call(&self, name: &str, args: Value) -> ToolOutput {
            self.tools.call(&self.conversation, name, args).await
        }

        /// Makes the call, answers its approval request with `decision`, and gives the result.
        async fn call_answering(&self, name: &str, args: Value, decision: Decision) -> ToolOutput {
            let (tools, conversation, name) = (Arc::clone(&self.tools), Arc::clone(&self.conversation), name.to_string());
            let call = tokio::spawn(async move { tools.call(&conversation, &name, args).await });
            let mut answered = false;
            for _ in 0..200 {
                if let Some(request) = self.sink.approvals().last() {
                    answered = self.tools.approvals.answer(&request.id, decision);
                    if answered {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(answered, "no approval was asked");
            call.await.unwrap()
        }
    }

    fn json_of(output: &ToolOutput) -> Value {
        serde_json::from_str(&output.text).unwrap_or_else(|e| panic!("not JSON ({e}): {}", output.text))
    }

    #[tokio::test]
    async fn list_tables_needs_no_approval() {
        let f = fixture(options(true, false)).await;
        let out = f.call("list_tables", json!({})).await;
        assert!(!out.is_error);
        assert!(f.sink.approvals().is_empty());
        let tables = json_of(&out);
        assert_eq!(tables["schemas"][0]["name"], "public");
        assert_eq!(tables["schemas"][0]["tables"], json!([{"name": "orders", "kind": "table"}, {"name": "recent", "kind": "view"}]));
        let only_audit = json_of(&f.call("list_tables", json!({"schema": "audit"})).await);
        assert_eq!(only_audit["schemas"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn describe_table_and_relationships_read_the_catalog() {
        let f = fixture(options(true, false)).await;
        let table = json_of(&f.call("describe_table", json!({"schema": "public", "table": "orders"})).await);
        assert_eq!(table["columns"][0]["name"], "id");
        assert_eq!(table["columns"][0]["primaryKey"], true);
        assert_eq!(table["comment"], "Customer orders");
        assert_eq!(table["foreignKeys"][0]["refTable"], "users");
        let relations = json_of(&f.call("relationships", json!({"schema": "public"})).await);
        assert_eq!(relations["relations"], json!(["public.orders(user_id) -> public.users(id)"]));
        assert!(f.sink.approvals().is_empty());
    }

    #[tokio::test]
    async fn run_query_asks_and_runs_once() {
        let f = fixture(options(true, false)).await;
        let out = f.call_answering("run_query", json!({"sql": "SELECT id, name FROM users"}), Decision::Once).await;
        assert!(!out.is_error, "{}", out.text);
        assert_eq!(f.db.reads(), vec!["SELECT id, name FROM users"]);
        let result = json_of(&out);
        assert_eq!((result["rowCount"].clone(), result["rows"][1][1].clone()), (json!(2), json!("bo")));
        let request = f.sink.approvals().remove(0);
        assert_eq!((request.tool.as_str(), request.sql.as_str(), request.analyze), ("run_query", "SELECT id, name FROM users", false));
        // A second one asks again
        f.call_answering("run_query", json!({"sql": "SELECT 2"}), Decision::Once).await;
        assert_eq!(f.sink.approvals().len(), 2);
    }

    #[tokio::test]
    async fn reads_in_chat_stops_asking() {
        let f = fixture(options(true, false)).await;
        f.call_answering("run_query", json!({"sql": "SELECT 1"}), Decision::ReadsInChat).await;
        let out = f.call("run_query", json!({"sql": "SELECT 2"})).await;
        assert!(!out.is_error);
        assert_eq!(f.sink.approvals().len(), 1);
        assert_eq!(f.db.reads().len(), 2);
    }

    #[tokio::test]
    async fn ask_off_never_asks() {
        let f = fixture(options(false, false)).await;
        let out = f.call("run_query", json!({"sql": "SELECT 1"})).await;
        assert!(!out.is_error && f.sink.approvals().is_empty());
    }

    #[tokio::test]
    async fn declined_query_tells_the_agent() {
        let f = fixture(options(true, false)).await;
        let out = f.call_answering("run_query", json!({"sql": "SELECT 1"}), Decision::Decline).await;
        assert!(out.is_error);
        assert_eq!(out.text, DECLINED);
        assert!(f.db.reads().is_empty());
    }

    #[tokio::test]
    async fn writing_sql_is_refused_without_asking() {
        let f = fixture(options(true, false)).await;
        let out = f.call("run_query", json!({"sql": "DELETE FROM t"})).await;
        assert!(out.is_error);
        assert_eq!(out.text, READ_REFUSED);
        assert!(f.sink.approvals().is_empty() && f.db.reads().is_empty());
        let several = f.call("run_query", json!({"sql": "SELECT 1; SELECT 2"})).await;
        assert_eq!(several.text, "Run one statement at a time.");
    }

    #[tokio::test]
    async fn schema_only_returns_no_values() {
        let f = fixture(options(false, true)).await;
        let out = f.call("run_query", json!({"sql": "SELECT id, name FROM users"})).await;
        let result = json_of(&out);
        assert_eq!(result["valuesHidden"], true);
        assert!(result.get("rows").is_none());
        assert_eq!(result["rowCount"], 2);
    }

    #[tokio::test]
    async fn explain_without_analyze_needs_no_approval() {
        let f = fixture(options(true, false)).await;
        let out = f.call("explain_query", json!({"sql": "SELECT * FROM t", "analyze": false})).await;
        assert!(!out.is_error, "{}", out.text);
        assert!(f.sink.approvals().is_empty());
        assert_eq!(f.db.explained.lock().unwrap().clone(), vec![("SELECT * FROM t".to_string(), false)]);
        assert_eq!(json_of(&out)["plan"][0]["Plan"]["Node Type"], "Seq Scan");
    }

    #[tokio::test]
    async fn explain_analyze_asks_and_is_blocked_in_schema_only() {
        let f = fixture(options(true, false)).await;
        let out = f.call_answering("explain_query", json!({"sql": "SELECT * FROM t", "analyze": true}), Decision::Once).await;
        assert!(!out.is_error, "{}", out.text);
        assert!(f.sink.approvals()[0].analyze);
        // It runs where reads run (a read-only transaction), not on the editor's connection
        assert_eq!(f.db.analyzed.lock().unwrap().clone(), vec!["SELECT * FROM t".to_string()]);
        assert!(f.db.explained.lock().unwrap().is_empty());
        assert_eq!(json_of(&out)["plan"][0]["Plan"]["Node Type"], "Seq Scan");

        let hidden = fixture(options(true, true)).await;
        let blocked = hidden.call("explain_query", json!({"sql": "SELECT 1", "analyze": true})).await;
        assert!(blocked.is_error);
        assert_eq!(blocked.text, SCHEMA_ONLY);
        assert!(hidden.db.explained.lock().unwrap().is_empty() && hidden.db.analyzed.lock().unwrap().is_empty());
        // And it never runs a statement that writes
        let writes = f.call("explain_query", json!({"sql": "DELETE FROM t", "analyze": true})).await;
        assert_eq!(writes.text, READ_REFUSED);
    }

    #[tokio::test]
    async fn closed_session_says_so() {
        let f = fixture(options(true, false)).await;
        f.tools.sessions.lock().await.clear();
        let out = f.call("list_tables", json!({})).await;
        assert!(out.is_error);
        assert_eq!(out.text, CLOSED);
    }

    #[tokio::test]
    async fn tokens_route_to_their_own_session() {
        let first = fixture(options(false, false)).await;
        let second_db = FakeDb::new();
        first.tools.sessions.lock().await.insert("s2".into(), second_db.clone());
        let other = Conversations::default().create("s2", options(false, false));
        first.tools.call(&other, "run_query", json!({"sql": "SELECT 'two'"})).await;
        first.call("run_query", json!({"sql": "SELECT 'one'"})).await;
        assert_eq!(second_db.reads(), vec!["SELECT 'two'"]);
        assert_eq!(first.db.reads(), vec!["SELECT 'one'"]);
    }

    #[tokio::test]
    async fn unknown_tool_and_missing_argument_are_errors() {
        let f = fixture(options(false, false)).await;
        assert!(f.call("drop_everything", json!({})).await.is_error);
        let missing = f.call("run_query", json!({})).await;
        assert!(missing.is_error && missing.text.contains("sql"));
    }

    #[test]
    fn definitions_have_five_tools() {
        let names: Vec<&str> = definitions().iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["list_tables", "describe_table", "relationships", "run_query", "explain_query"]);
        assert!(definitions().iter().all(|d| !d.description.is_empty() && d.input_schema["type"] == "object"));
    }
}
