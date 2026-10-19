//! Tests against the real adapter and a signed-in Claude Code. They need Node 20+ and the
//! person's login, and use their subscription, so they only run with `DBUNNY_TEST_CLAUDE=1`:
//!
//!   DBUNNY_TEST_CLAUDE=1 cargo test --lib claude -- --ignored --nocapture

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::agent::{AgentHandle, SessionSetup};
use super::approvals::Approvals;
use super::conversation::{Conversation, ConversationOptions, Conversations};
use super::events::{AssistantEvent, EventSink, MemorySink};
use super::launch::{check_node, default_command, login_shell_path};
use super::mcp::McpServer;
use super::tools::Tools;
use crate::db::sqlite::{OpenOptions, SqliteDriver};
use crate::db::Driver;

struct Live {
    agent: Arc<AgentHandle>,
    server: McpServer,
    conversations: Arc<Conversations>,
    sink: Arc<MemorySink>,
    conversation: Arc<Conversation>,
    setup: SessionSetup,
    file: std::path::PathBuf,
}

fn enabled() -> bool {
    std::env::var("DBUNNY_TEST_CLAUDE").is_ok_and(|v| v == "1")
}

impl Live {
    async fn start() -> Live {
        let file = std::env::temp_dir().join(format!("dbunny-test-claude-{}.sqlite", uuid::Uuid::new_v4()));
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch("CREATE TABLE t (a INTEGER, b TEXT); INSERT INTO t VALUES (1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e'),(6,'f'),(7,'g');")
            .unwrap();
        let driver: Arc<dyn Driver> = SqliteDriver::open(file.clone(), OpenOptions::default()).await.unwrap();
        let sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>> = Default::default();
        sessions.lock().await.insert("s1".into(), driver);

        let (sink, conversations, approvals) = (MemorySink::new(), Arc::new(Conversations::default()), Arc::new(Approvals::default()));
        let events: Arc<dyn EventSink> = sink.clone();
        let tools = Arc::new(Tools { sessions, approvals: Arc::clone(&approvals), events: Arc::clone(&events) });
        let server = McpServer::start(Arc::clone(&conversations), tools).await.unwrap();

        let path = login_shell_path().await;
        check_node(path.as_deref()).await.expect("Node 20 or newer is needed");
        let agent = AgentHandle::spawn(&default_command(cfg!(windows)), path, events, Arc::clone(&conversations), approvals)
            .await
            .unwrap_or_else(|status| panic!("the agent didn't start: {status:?}"));

        let options = ConversationOptions { ask_before_queries: false, schema_only: false, row_limit: 100, instructions: String::new() };
        let conversation = conversations.create("s1", options);
        let cwd = std::env::temp_dir().join(format!("dbunny-test-claude-cwd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&cwd).unwrap();
        let setup = SessionSetup {
            cwd,
            mcp_url: server.url(),
            token: conversation.token.clone(),
            system_prompt: "You are the SQL assistant inside DBunny. The database is SQLite. Use the tools to look at the data, and answer in one short sentence.".into(),
        };
        Live { agent, server, conversations, sink, conversation, setup, file }
    }

    /// The text the agent wrote to the conversation, and the names of the tools it called.
    fn heard(&self) -> (String, Vec<String>) {
        let (mut text, mut tools) = (String::new(), Vec::new());
        for event in self.sink.events() {
            let AssistantEvent::Update { conversation_id, update } = event else { continue };
            if conversation_id != self.conversation.id {
                continue;
            }
            match update["sessionUpdate"].as_str() {
                Some("agent_message_chunk") => text.push_str(update["content"]["text"].as_str().unwrap_or("")),
                Some("tool_call") => {
                    if let Some(name) = update["_meta"]["claudeCode"]["toolName"].as_str() {
                        tools.push(name.to_string());
                    }
                }
                _ => {}
            }
        }
        (text, tools)
    }

    async fn finish(self) {
        self.agent.shutdown().await;
        self.server.stop().await;
        let _ = std::fs::remove_file(&self.file);
        let _ = std::fs::remove_dir_all(&self.setup.cwd);
        drop(self.conversations);
    }
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CLAUDE=1, Node 20+ and a signed-in Claude Code"]
async fn claude_session_sees_only_dbunny_tools() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    let meta = {
        let mut meta = super::agent::session_meta();
        meta["claudeCode"]["emitRawSDKMessages"] = json!(true);
        meta
    };
    let opened = live.agent.new_session_with_meta(&live.setup, meta).await.unwrap();
    *live.conversation.acp_session.lock().unwrap() = Some(opened.acp_session.clone());
    // The session reports what it has when its first message starts; the answer doesn't matter
    let _ = live.agent.prompt(&opened.acp_session, "Say hello.").await;
    let mut init = None;
    for _ in 0..100 {
        init = live.agent.extension_messages().into_iter().map(|m| m["params"]["message"].clone()).find(|m| m["type"] == "system" && m["subtype"] == "init");
        if init.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let init = init.expect("the agent reported no init message");
    let tools: Vec<&str> = init["tools"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    println!("tools: {tools:?}");
    assert_eq!(tools.len(), 5, "{tools:?}");
    assert!(tools.iter().all(|t| t.starts_with("mcp__dbunny__")), "{tools:?}");
    let servers = init["mcp_servers"].as_array().unwrap();
    assert!(servers.iter().all(|s| s["name"] == "dbunny"), "{servers:?}");
    live.finish().await;
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CLAUDE=1, Node 20+ and a signed-in Claude Code"]
async fn claude_answers_with_run_query_and_keeps_history() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    let opened = live.agent.new_session(&live.setup).await.unwrap();
    let id = opened.acp_session.clone();
    println!("models: {:?}", opened.models);
    *live.conversation.acp_session.lock().unwrap() = Some(id.clone());
    live.agent.prompt(&id, "How many rows does the table t have? Use the tools to find out, then reply with just the number.").await.unwrap();
    let (text, tools) = live.heard();
    println!("answer: {text:?}\ntools: {tools:?}");
    assert!(tools.iter().any(|t| t == "mcp__dbunny__run_query"), "{tools:?}");
    assert!(text.contains('7'), "{text}");

    // The conversation can be listed for its folder, and only for it
    let listed = live.agent.list_sessions(&live.setup.cwd).await.unwrap();
    println!("sessions: {listed:?}");
    assert!(listed.iter().any(|s| s.acp_session == id), "{listed:?}");
    let elsewhere = live.agent.list_sessions(&std::env::temp_dir().join("dbunny-test-claude-nowhere")).await.unwrap();
    assert!(elsewhere.iter().all(|s| s.acp_session != id));

    // And reopened, it plays back what was said
    let before = live.sink.events().len();
    let mut again = SessionSetup { cwd: live.setup.cwd.clone(), mcp_url: live.setup.mcp_url.clone(), token: live.setup.token.clone(), system_prompt: live.setup.system_prompt.clone() };
    again.token = live.conversation.token.clone();
    live.agent.load_session(&id, &again).await.unwrap();
    let replayed = live.sink.events().into_iter().skip(before).filter(|e| matches!(e, AssistantEvent::Update { update, .. } if update["sessionUpdate"] == "user_message_chunk")).count();
    assert!(replayed >= 1, "nothing was played back");

    live.finish().await;
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CLAUDE=1, Node 20+ and a signed-in Claude Code"]
async fn claude_reports_what_it_offers() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    let opened = live.agent.new_session(&live.setup).await.unwrap();
    println!("models: {:?}", opened.models);
    println!("system prompt capability: {}", live.agent.supports_system_prompt());
    live.agent.set_model(&opened.acp_session, "haiku").await.unwrap();
    live.finish().await;
}
