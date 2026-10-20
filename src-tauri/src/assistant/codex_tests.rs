//! Tests against the real Codex adapter and a signed-in Codex. They need Node 20+ and the person's
//! login, and use their account, so they only run with `DBUNNY_TEST_CODEX=1`, one at a time:
//!
//!   DBUNNY_TEST_CODEX=1 cargo test --lib codex_live -- --ignored --nocapture --test-threads=1

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use super::agent::is_own_tool_call;
use super::conversation::ConversationOptions;
use super::events::{AgentStatus, AssistantEvent, EventSink, MemorySink};
use super::profile::AgentKind;
use super::{Assistant, SystemHost};
use crate::db::sqlite::{OpenOptions, SqliteDriver};
use crate::db::Driver;

fn enabled() -> bool {
    std::env::var("DBUNNY_TEST_CODEX").is_ok_and(|v| v == "1")
}

struct Live {
    assistant: Assistant,
    sink: Arc<MemorySink>,
    data: PathBuf,
    file: PathBuf,
}

impl Live {
    async fn start() -> Live {
        Live::start_with_login(true).await
    }

    /// `login`: DBunny's Codex home gets a link to the person's own Codex login for the test (read only,
    /// no test signs out); without it Codex is signed out, as it is the first time in the app.
    async fn start_with_login(login: bool) -> Live {
        // The real path of the temporary folder: the agent lists a conversation by the folder it ran in
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let file = base.join(format!("dbunny-test-codex-{}.sqlite", uuid::Uuid::new_v4()));
        rusqlite::Connection::open(&file)
            .unwrap()
            .execute_batch("CREATE TABLE t (a INTEGER, b TEXT); INSERT INTO t VALUES (1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e'),(6,'f'),(7,'g');")
            .unwrap();
        let driver: Arc<dyn Driver> = SqliteDriver::open(file.clone(), OpenOptions::default()).await.unwrap();
        let sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>> = Default::default();
        sessions.lock().await.insert("s1".into(), driver);
        let sink = MemorySink::new();
        let events: Arc<dyn EventSink> = sink.clone();
        let data = base.join(format!("dbunny-test-codex-data-{}", uuid::Uuid::new_v4()));
        if login {
            let home = data.join("codex-home");
            std::fs::create_dir_all(&home).unwrap();
            let own = PathBuf::from(std::env::var("HOME").unwrap()).join(".codex/auth.json");
            #[cfg(unix)]
            std::os::unix::fs::symlink(own, home.join("auth.json")).unwrap();
            #[cfg(not(unix))]
            std::fs::copy(own, home.join("auth.json")).unwrap();
        }
        let assistant = Assistant::with_host(data.clone(), sessions, events, Arc::new(SystemHost));
        Live { assistant, sink, data, file }
    }

    async fn ready(&self) {
        assert_eq!(self.assistant.start(AgentKind::Codex, None).await, AgentStatus::Ready, "{:?}", self.assistant.log().await);
    }

    fn options(schema_only: bool) -> ConversationOptions {
        ConversationOptions { ask_before_queries: false, schema_only, row_limit: 100, instructions: String::new() }
    }

    /// The agent's updates for one conversation.
    fn updates(&self, conversation: &str) -> Vec<Value> {
        self.sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                AssistantEvent::Update { conversation_id, update } if conversation_id == conversation => Some(update),
                _ => None,
            })
            .collect()
    }

    fn said(&self, conversation: &str) -> String {
        self.updates(conversation).iter().filter(|u| u["sessionUpdate"] == "agent_message_chunk").filter_map(|u| u["content"]["text"].as_str()).collect()
    }

    async fn finish(self) {
        self.assistant.shutdown().await;
        let _ = std::fs::remove_file(&self.file);
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CODEX=1, Node 20+ and a signed-in Codex"]
async fn codex_live_cannot_use_its_own_tools() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    live.ready().await;
    let opened = live.assistant.new_conversation("s1", Some("saved-codex"), Live::options(false)).await.unwrap();
    for ask in [
        "Run the shell command `cat /etc/hosts` and tell me its first line. If you can't, say CANNOT.",
        "Read the file /etc/hosts with any tool you have and tell me its first line. If you can't, say CANNOT.",
        "Search the web for the weather in Paris. If you can't, say CANNOT.",
    ] {
        live.assistant.prompt(&opened.conversation_id, ask).await.unwrap();
    }
    let calls: Vec<Value> = live.updates(&opened.conversation_id).into_iter().filter(|u| u["sessionUpdate"] == "tool_call").collect();
    println!("said: {:?}\ncalls: {:?}", live.said(&opened.conversation_id), calls.iter().map(|c| c["title"].clone()).collect::<Vec<_>>());
    // An attempt may show (the agent tries); what matters is that nothing but the database's tools ran
    // (Codex's own listing of the resources of MCP servers is no tool of use: there are none to list)
    let benign = |call: &Value| call["rawInput"]["server"] == "codex" && call["rawInput"]["tool"].as_str().is_some_and(|tool| tool.contains("mcp_resource"));
    let foreign: Vec<&str> = calls.iter().filter(|c| !is_own_tool_call(c) && !benign(c)).filter_map(|c| c["toolCallId"].as_str()).collect();
    let ran = live
        .updates(&opened.conversation_id)
        .iter()
        .filter(|u| u["sessionUpdate"] == "tool_call_update" && u["status"] == "completed" && foreign.contains(&u["toolCallId"].as_str().unwrap_or("")))
        .count();
    println!("tools tried that aren't the database's: {foreign:?}");
    for u in live.updates(&opened.conversation_id).iter().filter(|u| foreign.contains(&u["toolCallId"].as_str().unwrap_or(""))) {
        println!("FOREIGN {}", u.to_string().chars().take(900).collect::<String>());
    }
    assert_eq!(ran, 0, "a tool that isn't the database's ran: {foreign:?}");
    live.finish().await;
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CODEX=1, Node 20+ and a signed-in Codex"]
async fn codex_live_answers_from_the_database_and_keeps_history() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    live.ready().await;
    let opened = live.assistant.new_conversation("s1", Some("saved-codex"), Live::options(false)).await.unwrap();
    live.assistant.prompt(&opened.conversation_id, "How many rows does the table t have? Use the tools to find out, then reply with just the number.").await.unwrap();
    let updates = live.updates(&opened.conversation_id);
    let queries: Vec<&Value> = updates.iter().filter(|u| u["sessionUpdate"] == "tool_call" && u["rawInput"]["tool"] == "run_query").collect();
    println!("said: {:?}\nqueries: {queries:?}", live.said(&opened.conversation_id));
    assert!(!queries.is_empty(), "it never ran a query");
    assert!(live.said(&opened.conversation_id).contains('7'));
    // The database context rode on the first message and was never shown by the agent
    assert!(!live.said(&opened.conversation_id).contains("dbunny-context"));

    // Listed by the folder of the connection, and reopened with its past messages, context included
    let listed = live.assistant.list_conversations("s1", Some("saved-codex")).await.unwrap();
    println!("listed: {listed:?}");
    assert!(listed.iter().any(|s| s.acp_session == opened.acp_session_id), "{listed:?}");
    // The person leaves the conversation before opening a past one, as the interface does
    live.assistant.close_conversation(&opened.conversation_id).await;
    let before = live.sink.events().len();
    let again = live.assistant.load_conversation("s1", Some("saved-codex"), &opened.acp_session_id, Live::options(false)).await.unwrap();
    let replayed: Vec<String> = live
        .sink
        .events()
        .into_iter()
        .skip(before)
        .filter_map(|e| match e {
            AssistantEvent::Update { conversation_id, update } if conversation_id == again.conversation_id && update["sessionUpdate"] == "user_message_chunk" => {
                update["content"]["text"].as_str().map(str::to_string)
            }
            _ => None,
        })
        .collect();
    println!("replayed: {replayed:?}");
    let kinds: Vec<String> = live.sink.events().into_iter().skip(before).filter_map(|e| match e { AssistantEvent::Update { update, .. } => update["sessionUpdate"].as_str().map(str::to_string), _ => None }).collect();
    println!("kinds after load: {kinds:?}");
    assert!(replayed.iter().any(|text| text.contains("<dbunny-context>")), "the replay should carry the context for the interface to hide");
    live.finish().await;
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CODEX=1, Node 20+ and a signed-in Codex"]
async fn codex_live_does_not_try_to_read_rows_when_values_are_hidden() {
    if !enabled() {
        return;
    }
    let live = Live::start().await;
    live.ready().await;
    let opened = live.assistant.new_conversation("s1", Some("saved-codex"), Live::options(true)).await.unwrap();
    live.assistant.prompt(&opened.conversation_id, "Me mostra 3 linhas da tabela t").await.unwrap();
    let ran: Vec<String> = live
        .updates(&opened.conversation_id)
        .iter()
        .filter(|u| u["sessionUpdate"] == "tool_call" && u["rawInput"]["tool"] == "run_query")
        .filter_map(|u| u["rawInput"]["arguments"]["sql"].as_str().map(str::to_string))
        .collect();
    println!("said: {:?}\nran: {ran:?}", live.said(&opened.conversation_id));
    assert!(ran.iter().all(|sql| sql.to_lowercase().contains("count")), "it ran a query that reads rows: {ran:?}");
    live.finish().await;
}

#[tokio::test]
#[ignore = "needs DBUNNY_TEST_CODEX=1 and Node 20+; Codex has no login here, and none is started"]
async fn codex_live_signed_out_asks_to_sign_in() {
    if !enabled() {
        return;
    }
    let live = Live::start_with_login(false).await;
    live.ready().await;
    let err = live.assistant.new_conversation("s1", Some("saved-codex"), Live::options(false)).await.err().unwrap();
    println!("error: {err}");
    match live.assistant.status() {
        AgentStatus::NeedsSignIn { methods, command } => {
            assert!(methods.iter().any(|m| m.id == "chat-gpt"), "{methods:?}");
            assert_eq!(command, None);
        }
        other => panic!("expected a sign-in, got {other:?}"),
    }
    live.finish().await;
}
