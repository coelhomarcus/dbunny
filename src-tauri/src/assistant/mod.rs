//! The AI assistant: the database tools an agent may use, how a conversation is kept, and
//! what the agent is told about the database.

pub mod agent;
pub mod approvals;
#[cfg(test)]
mod claude_tests;
pub mod context;
pub mod conversation;
pub mod events;
pub mod launch;
pub mod mcp;
#[cfg(test)]
mod fake;
#[cfg(test)]
mod fake_agent;
pub mod result;
pub mod tools;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Mutex;

use agent::{AgentError, AgentHandle, ModelChoice, SessionSetup, SessionSummary};
use approvals::{Approvals, Decision};
use context::{system_prompt, PromptInput};
use conversation::{Conversation, ConversationOptions, Conversations, Workspace};
use events::{AgentStatus, AssistantEvent, EventSink};
use launch::{default_command, login_command, parse_custom, AgentCommand};
use mcp::McpServer;
use tools::Tools;

use crate::db::model::CompletionSchema;
use crate::db::Driver;

/// Where the person's conversations with one connection (and one of its databases) are kept,
/// and whether the folder goes away with the conversation (the connection wasn't saved).
pub fn workspace_dir(data_dir: &Path, saved_id: Option<&str>, database: &str) -> (PathBuf, bool) {
    match saved_id.filter(|id| is_safe_id(id)) {
        Some(id) => {
            let hex: String = database.bytes().map(|byte| format!("{byte:02x}")).collect();
            (data_dir.join("assistant").join(id).join(format!("db-{hex}")), false)
        }
        None => (data_dir.join("assistant").join("unsaved").join(uuid::Uuid::new_v4().to_string()), true),
    }
}

/// Saved connections are named by ids made of letters, digits and dashes; anything else must
/// not become part of a path.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// What the assistant needs from the machine, so that tests can do without Node and npx.
#[async_trait]
pub trait AgentHost: Send + Sync {
    async fn login_path(&self) -> Option<String>;
    async fn check_node(&self, path: Option<&str>) -> Result<(u32, u32, u32), Option<String>>;
    async fn launch(
        &self,
        command: &AgentCommand,
        path: Option<String>,
        events: Arc<dyn EventSink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    ) -> Result<Arc<AgentHandle>, AgentStatus>;
}

/// This computer: its login shell, its Node and the real agent process.
pub struct SystemHost;

#[async_trait]
impl AgentHost for SystemHost {
    async fn login_path(&self) -> Option<String> {
        launch::login_shell_path().await
    }

    async fn check_node(&self, path: Option<&str>) -> Result<(u32, u32, u32), Option<String>> {
        launch::check_node(path).await
    }

    async fn launch(
        &self,
        command: &AgentCommand,
        path: Option<String>,
        events: Arc<dyn EventSink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    ) -> Result<Arc<AgentHandle>, AgentStatus> {
        AgentHandle::spawn(command, path, events, conversations, approvals).await
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpenedConversation {
    pub conversation_id: String,
    pub acp_session_id: String,
    pub models: Option<ModelChoice>,
}

/// Passes the events on and remembers the last status.
struct StatusTap {
    inner: Arc<dyn EventSink>,
    status: StdMutex<AgentStatus>,
}

impl EventSink for StatusTap {
    fn emit(&self, event: AssistantEvent) {
        if let AssistantEvent::Status { status } = &event {
            *self.status.lock().unwrap() = status.clone();
        }
        self.inner.emit(event);
    }
}

struct Running {
    agent: Arc<AgentHandle>,
    mcp: McpServer,
}

/// The assistant of the app: one agent and one tool server, started on first use, and the
/// conversations people hold with it.
pub struct Assistant {
    data_dir: PathBuf,
    sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>>,
    tap: Arc<StatusTap>,
    host: Arc<dyn AgentHost>,
    conversations: Arc<Conversations>,
    approvals: Arc<Approvals>,
    tools: Arc<Tools>,
    running: Mutex<Option<Running>>,
    /// The agent is one the person chose, so how to sign in to it is not for DBunny to say.
    custom_command: AtomicBool,
}

impl Assistant {
    pub fn new(data_dir: PathBuf, sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>>, events: Arc<dyn EventSink>) -> Assistant {
        Assistant::with_host(data_dir, sessions, events, Arc::new(SystemHost))
    }

    pub fn with_host(data_dir: PathBuf, sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>>, events: Arc<dyn EventSink>, host: Arc<dyn AgentHost>) -> Assistant {
        let tap = Arc::new(StatusTap { inner: events, status: StdMutex::new(AgentStatus::Idle) });
        let approvals = Arc::new(Approvals::default());
        let tools = Arc::new(Tools { sessions: Arc::clone(&sessions), approvals: Arc::clone(&approvals), events: tap.clone() });
        Assistant { data_dir, sessions, tap, host, conversations: Arc::new(Conversations::default()), approvals, tools, running: Mutex::new(None), custom_command: AtomicBool::new(false) }
    }

    pub fn status(&self) -> AgentStatus {
        self.tap.status.lock().unwrap().clone()
    }

    fn set_status(&self, status: AgentStatus) {
        self.tap.emit(AssistantEvent::Status { status });
    }

    /// Brings the agent and the tool server up, unless they already are. `command` is the
    /// person's own command, if they set one.
    pub async fn start(&self, command: Option<String>) -> AgentStatus {
        let mut running = self.running.lock().await;
        if running.is_some() && self.status() == AgentStatus::Ready {
            return AgentStatus::Ready;
        }
        if let Some(old) = running.take() {
            old.agent.shutdown().await;
            old.mcp.stop().await;
        }
        self.set_status(AgentStatus::Starting);
        let path = self.host.login_path().await;
        if let Err(found) = self.host.check_node(path.as_deref()).await {
            return self.fail(AgentStatus::NeedsNode { found });
        }
        let own_command = command.as_deref().map(str::trim).filter(|text| !text.is_empty());
        self.custom_command.store(own_command.is_some(), Ordering::SeqCst);
        let command = match own_command {
            Some(text) => match parse_custom(text) {
                Ok(command) => command,
                Err(message) => return self.fail(AgentStatus::Failed { message }),
            },
            None => default_command(cfg!(windows)),
        };
        let mcp = match McpServer::start(Arc::clone(&self.conversations), Arc::clone(&self.tools)).await {
            Ok(mcp) => mcp,
            Err(message) => return self.fail(AgentStatus::Failed { message }),
        };
        let events: Arc<dyn EventSink> = self.tap.clone();
        match self.host.launch(&command, path, events, Arc::clone(&self.conversations), Arc::clone(&self.approvals)).await {
            Ok(agent) => {
                *running = Some(Running { agent, mcp });
                self.set_status(AgentStatus::Ready);
                AgentStatus::Ready
            }
            Err(status) => {
                mcp.stop().await;
                self.fail(status)
            }
        }
    }

    fn fail(&self, status: AgentStatus) -> AgentStatus {
        self.set_status(status.clone());
        status
    }

    pub async fn restart(&self, command: Option<String>) -> AgentStatus {
        self.shutdown().await;
        self.start(command).await
    }

    /// Ends the agent and the tool server.
    pub async fn shutdown(&self) {
        let old = self.running.lock().await.take();
        if let Some(old) = old {
            old.agent.shutdown().await;
            old.mcp.stop().await;
            self.set_status(AgentStatus::Idle);
        }
    }

    pub async fn mcp_url(&self) -> Option<String> {
        self.running.lock().await.as_ref().map(|running| running.mcp.url())
    }

    async fn agent(&self) -> Result<Arc<AgentHandle>, String> {
        if self.status() != AgentStatus::Ready {
            return Err("The assistant isn't running".into());
        }
        self.running.lock().await.as_ref().map(|running| Arc::clone(&running.agent)).ok_or_else(|| "The assistant isn't running".to_string())
    }

    pub async fn new_conversation(&self, session_id: &str, saved_id: Option<&str>, options: ConversationOptions) -> Result<OpenedConversation, String> {
        self.open(session_id, saved_id, options, None).await
    }

    pub async fn load_conversation(&self, session_id: &str, saved_id: Option<&str>, acp_session: &str, options: ConversationOptions) -> Result<OpenedConversation, String> {
        self.open(session_id, saved_id, options, Some(acp_session)).await
    }

    async fn open(&self, session_id: &str, saved_id: Option<&str>, options: ConversationOptions, resume: Option<&str>) -> Result<OpenedConversation, String> {
        let agent = self.agent().await?;
        let driver = self.sessions.lock().await.get(session_id).cloned().ok_or_else(|| "Session not found".to_string())?;
        let info = driver.info();
        let schema = driver.catalog().completion_schema().await.unwrap_or(CompletionSchema { current_schema: None, relations: vec![], functions: vec![] });
        let (cwd, temporary) = workspace_dir(&self.data_dir, saved_id, &info.database);
        std::fs::create_dir_all(&cwd).map_err(|e| format!("Couldn't prepare the assistant's folder: {e}"))?;
        let prompt = system_prompt(&PromptInput { engine: driver.engine(), info: &info, schema: &schema, instructions: &options.instructions, schema_only: options.schema_only });
        let mcp_url = self.mcp_url().await.ok_or_else(|| "The assistant isn't running".to_string())?;

        let conversation = self.conversations.create(session_id, options);
        *conversation.workspace.lock().unwrap() = Some(Workspace { path: cwd.clone(), temporary });
        let setup = SessionSetup { cwd, mcp_url, token: conversation.token.clone(), system_prompt: prompt };
        let outcome = match resume {
            Some(acp_session) => {
                // The replay of the past messages arrives before the load is answered
                *conversation.acp_session.lock().unwrap() = Some(acp_session.to_string());
                agent.load_session(acp_session, &setup).await
            }
            None => agent.new_session(&setup).await,
        };
        match outcome {
            Ok(opened) => {
                *conversation.acp_session.lock().unwrap() = Some(opened.acp_session.clone());
                Ok(OpenedConversation { conversation_id: conversation.id.clone(), acp_session_id: opened.acp_session, models: opened.models })
            }
            Err(e) => {
                self.conversations.remove(&conversation.id);
                self.forget_workspace(&conversation);
                Err(self.explain(e))
            }
        }
    }

    /// The agent's failure as a message; one that asks for a sign-in also changes the status.
    fn explain(&self, error: AgentError) -> String {
        if let AgentError::AuthRequired(methods) = &error {
            let command = (!self.custom_command.load(Ordering::SeqCst)).then(login_command);
            self.set_status(AgentStatus::NeedsSignIn { methods: methods.clone(), command });
        }
        error.to_string()
    }

    pub async fn list_conversations(&self, session_id: &str, saved_id: Option<&str>) -> Result<Vec<SessionSummary>, String> {
        let Some(saved_id) = saved_id else { return Ok(vec![]) };
        let agent = self.agent().await?;
        let driver = self.sessions.lock().await.get(session_id).cloned().ok_or_else(|| "Session not found".to_string())?;
        let (cwd, _) = workspace_dir(&self.data_dir, Some(saved_id), &driver.info().database);
        agent.list_sessions(&cwd).await.map_err(|e| self.explain(e))
    }

    pub async fn prompt(&self, conversation_id: &str, text: &str) -> Result<String, String> {
        let conversation = self.conversations.get(conversation_id).ok_or_else(|| "This conversation was closed".to_string())?;
        let acp_session = conversation.acp_session.lock().unwrap().clone().ok_or_else(|| "This conversation has not started".to_string())?;
        let agent = self.agent().await?;
        agent.prompt(&acp_session, text).await.map_err(|e| self.explain(e))
    }

    /// Stops the turn and turns down what was waiting for the person's approval.
    pub async fn cancel(&self, conversation_id: &str) {
        let Some(conversation) = self.conversations.get(conversation_id) else { return };
        self.stop_turn(&conversation).await;
    }

    async fn stop_turn(&self, conversation: &Conversation) {
        self.approvals.decline_all(&conversation.id);
        let acp_session = conversation.acp_session.lock().unwrap().clone();
        let agent = self.running.lock().await.as_ref().map(|running| Arc::clone(&running.agent));
        if let (Some(agent), Some(acp_session)) = (agent, acp_session) {
            agent.cancel(&acp_session).await;
        }
    }

    pub fn approve(&self, approval_id: &str, decision: Decision) -> bool {
        self.approvals.answer(approval_id, decision)
    }

    pub async fn set_model(&self, conversation_id: &str, model_id: &str) -> Result<(), String> {
        let conversation = self.conversations.get(conversation_id).ok_or_else(|| "This conversation was closed".to_string())?;
        let acp_session = conversation.acp_session.lock().unwrap().clone().ok_or_else(|| "This conversation has not started".to_string())?;
        self.agent().await?.set_model(&acp_session, model_id).await.map_err(|e| self.explain(e))
    }

    pub async fn close_conversation(&self, conversation_id: &str) {
        if let Some(conversation) = self.conversations.remove(conversation_id) {
            self.stop_turn(&conversation).await;
            self.forget_workspace(&conversation);
        }
    }

    /// The database session is going away: its conversations end with it.
    pub async fn close_for_session(&self, session_id: &str) {
        for conversation in self.conversations.remove_for_session(session_id) {
            self.stop_turn(&conversation).await;
            self.forget_workspace(&conversation);
        }
    }

    fn forget_workspace(&self, conversation: &Conversation) {
        let workspace = conversation.workspace.lock().unwrap().clone();
        if let Some(Workspace { path, temporary: true }) = workspace {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    /// A saved connection was removed: so is the folder its conversations were kept in.
    pub fn forget_saved(&self, saved_id: &str) {
        if is_safe_id(saved_id) {
            let _ = std::fs::remove_dir_all(self.data_dir.join("assistant").join(saved_id));
        }
    }

    /// Signs out of Claude on this computer, which ends every conversation; the panel then asks to sign in.
    pub async fn sign_out(&self, command: Option<String>) -> Result<(), String> {
        let status = self.start(command).await;
        if status != AgentStatus::Ready {
            return Err(match status {
                AgentStatus::NeedsNode { .. } => "Node.js 20 or newer is needed".to_string(),
                AgentStatus::Failed { message } | AgentStatus::Unsupported { message } => message,
                _ => "The assistant isn't running".to_string(),
            });
        }
        self.agent().await?.logout().await.map_err(|e| e.to_string())?;
        // What was said is gone with the account: nothing of it can go on
        for conversation in self.conversations.all() {
            self.conversations.remove(&conversation.id);
            self.stop_turn(&conversation).await;
            self.forget_workspace(&conversation);
        }
        let command = (!self.custom_command.load(Ordering::SeqCst)).then(login_command);
        self.set_status(AgentStatus::NeedsSignIn { methods: vec![], command });
        Ok(())
    }

    pub async fn log(&self) -> Vec<String> {
        self.running.lock().await.as_ref().map(|running| running.agent.log()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::assistant::events::MemorySink;
    use crate::assistant::fake::FakeDb;
    use crate::assistant::fake_agent::{FakeAgent, FakeConfig};

    #[test]
    fn workspace_dirs() {
        let data = Path::new("/data");
        let hex = |name: &str| name.bytes().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(workspace_dir(data, Some("abc-1"), "shop"), (data.join("assistant/abc-1").join(format!("db-{}", hex("shop"))), false));
        let (unsaved, temporary) = workspace_dir(data, None, "shop");
        assert!(unsaved.starts_with(data.join("assistant/unsaved")) && temporary);
        let (hostile, temporary) = workspace_dir(data, Some("../x"), "shop");
        assert!(hostile.starts_with(data.join("assistant/unsaved")) && temporary);
        let (other, _) = workspace_dir(data, Some("abc-1"), "inventory");
        assert_ne!(other, workspace_dir(data, Some("abc-1"), "shop").0);
    }

    /// A host with no Node of its own, whose agent is a `FakeAgent`.
    struct FakeHost {
        node: Result<(u32, u32, u32), Option<String>>,
        config: StdMutex<FakeConfig>,
        agents: StdMutex<Vec<FakeAgent>>,
        launched: StdMutex<Vec<AgentCommand>>,
    }

    impl FakeHost {
        fn new(config: FakeConfig) -> Arc<FakeHost> {
            Arc::new(FakeHost { node: Ok((22, 14, 0)), config: StdMutex::new(config), agents: Default::default(), launched: Default::default() })
        }

        fn agent(&self) -> FakeAgent {
            self.agents.lock().unwrap().last().cloned().expect("no agent was started")
        }
    }

    #[async_trait]
    impl AgentHost for FakeHost {
        async fn login_path(&self) -> Option<String> {
            None
        }

        async fn check_node(&self, _path: Option<&str>) -> Result<(u32, u32, u32), Option<String>> {
            self.node.clone()
        }

        async fn launch(
            &self,
            command: &AgentCommand,
            _path: Option<String>,
            events: Arc<dyn EventSink>,
            conversations: Arc<Conversations>,
            approvals: Arc<Approvals>,
        ) -> Result<Arc<AgentHandle>, AgentStatus> {
            self.launched.lock().unwrap().push(command.clone());
            let (read, write, fake) = FakeAgent::start(self.config.lock().unwrap().clone());
            self.agents.lock().unwrap().push(fake);
            AgentHandle::over(read, write, events, conversations, approvals).await
        }
    }

    struct Rig {
        assistant: Arc<Assistant>,
        host: Arc<FakeHost>,
        sink: Arc<MemorySink>,
        data: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.data);
        }
    }

    fn options() -> ConversationOptions {
        ConversationOptions { ask_before_queries: true, schema_only: false, row_limit: 100, instructions: "status 3 means cancelled".into() }
    }

    async fn rig_with(host: Arc<FakeHost>) -> Rig {
        let data = std::env::temp_dir().join(format!("dbunny-test-assistant-{}", uuid::Uuid::new_v4()));
        let sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>> = Default::default();
        sessions.lock().await.insert("s1".into(), FakeDb::new());
        let sink = MemorySink::new();
        let events: Arc<dyn EventSink> = sink.clone();
        let assistant = Arc::new(Assistant::with_host(data.clone(), sessions, events, host.clone()));
        Rig { assistant, host, sink, data }
    }

    async fn rig(config: FakeConfig) -> Rig {
        rig_with(FakeHost::new(config)).await
    }

    async fn started(config: FakeConfig) -> Rig {
        let r = rig(config).await;
        assert_eq!(r.assistant.start(None).await, AgentStatus::Ready);
        r
    }

    #[tokio::test]
    async fn start_brings_the_agent_up_with_the_default_command() {
        let r = rig(FakeConfig::default()).await;
        assert_eq!(r.assistant.status(), AgentStatus::Idle);
        assert_eq!(r.assistant.start(None).await, AgentStatus::Ready);
        assert_eq!(r.assistant.status(), AgentStatus::Ready);
        assert_eq!(r.host.launched.lock().unwrap().clone(), vec![default_command(cfg!(windows))]);
        let states: Vec<AgentStatus> = r.sink.events().into_iter().filter_map(|e| match e { AssistantEvent::Status { status } => Some(status), _ => None }).collect();
        assert_eq!(states, vec![AgentStatus::Starting, AgentStatus::Ready]);
        // Starting again is a no-op
        assert_eq!(r.assistant.start(None).await, AgentStatus::Ready);
        assert_eq!(r.host.launched.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn start_takes_a_custom_command() {
        let r = rig(FakeConfig::default()).await;
        assert_eq!(r.assistant.start(Some("/opt/agent --flag".into())).await, AgentStatus::Ready);
        assert_eq!(r.host.launched.lock().unwrap()[0], AgentCommand { program: "/opt/agent".into(), args: vec!["--flag".into()] });
        let bad = rig(FakeConfig::default()).await;
        assert!(matches!(bad.assistant.start(Some("run \"unclosed".into())).await, AgentStatus::Failed { .. }));
    }

    #[tokio::test]
    async fn start_reports_needs_node() {
        let host = Arc::new(FakeHost { node: Err(Some("18.19.1".into())), config: StdMutex::new(FakeConfig::default()), agents: Default::default(), launched: Default::default() });
        let r = rig_with(host).await;
        assert_eq!(r.assistant.start(None).await, AgentStatus::NeedsNode { found: Some("18.19.1".into()) });
        assert!(r.host.launched.lock().unwrap().is_empty());
        let none = Arc::new(FakeHost { node: Err(None), config: StdMutex::new(FakeConfig::default()), agents: Default::default(), launched: Default::default() });
        assert_eq!(rig_with(none).await.assistant.start(None).await, AgentStatus::NeedsNode { found: None });
    }

    #[tokio::test]
    async fn new_conversation_builds_the_prompt_and_session() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        assert_eq!(opened.acp_session_id, "acp-1");
        assert!(opened.models.is_some());
        let params = r.host.agent().calls("session/new")[0]["params"].clone();
        let prompt = params["systemPrompt"]["content"][0]["text"].as_str().unwrap().to_string();
        assert!(prompt.contains("public.orders(id int4)") && prompt.contains("PostgreSQL 16.2") && prompt.contains("status 3 means cancelled"), "{prompt}");
        let cwd = PathBuf::from(params["cwd"].as_str().unwrap());
        assert!(cwd.is_dir() && cwd.starts_with(r.data.join("assistant/saved-1")), "{cwd:?}");
        assert_eq!(params["mcpServers"][0]["url"], r.assistant.mcp_url().await.unwrap());
        let token = params["mcpServers"][0]["headers"][0]["value"].as_str().unwrap().strip_prefix("Bearer ").unwrap().to_string();
        assert_eq!(r.assistant.conversations.by_token(&token).unwrap().id, opened.conversation_id);
    }

    #[tokio::test]
    async fn a_conversation_that_hides_values_tells_the_agent_so() {
        let r = started(FakeConfig::default()).await;
        r.assistant.new_conversation("s1", Some("saved-1"), ConversationOptions { schema_only: true, ..options() }).await.unwrap();
        let params = r.host.agent().calls("session/new")[0]["params"].clone();
        assert!(params["systemPrompt"]["content"][0]["text"].as_str().unwrap().contains("This connection hides values"));
        let shown = started(FakeConfig::default()).await;
        shown.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        let params = shown.host.agent().calls("session/new")[0]["params"].clone();
        assert!(!params["systemPrompt"]["content"][0]["text"].as_str().unwrap().contains("This connection hides values"));
    }

    #[tokio::test]
    async fn the_prompt_goes_in_meta_when_the_agent_has_no_system_prompt() {
        let r = started(FakeConfig { system_prompt: false, ..Default::default() }).await;
        r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        let params = r.host.agent().calls("session/new")[0]["params"].clone();
        assert!(params.get("systemPrompt").is_none());
        assert!(params["_meta"]["systemPrompt"].as_str().unwrap().contains("public.orders"));
    }

    #[tokio::test]
    async fn an_unsaved_connection_works_in_a_folder_that_goes_with_the_conversation() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", None, options()).await.unwrap();
        let cwd = PathBuf::from(r.host.agent().calls("session/new")[0]["params"]["cwd"].as_str().unwrap());
        assert!(cwd.is_dir() && cwd.starts_with(r.data.join("assistant/unsaved")));
        r.assistant.close_conversation(&opened.conversation_id).await;
        assert!(!cwd.exists());
    }

    #[tokio::test]
    async fn conversations_are_listed_for_the_folder_of_the_connection() {
        let sessions = vec![json!({"sessionId": "old", "cwd": "/x", "title": "Sales"})];
        let r = started(FakeConfig { sessions, ..Default::default() }).await;
        let listed = r.assistant.list_conversations("s1", Some("saved-1")).await.unwrap();
        assert_eq!(listed.len(), 1);
        let cwd = r.host.agent().calls("session/list")[0]["params"]["cwd"].as_str().unwrap().to_string();
        assert!(cwd.contains("assistant/saved-1/db-"), "{cwd}");
        // Nothing to list for a connection that was not saved
        assert!(r.assistant.list_conversations("s1", None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_reopened_conversation_gets_its_past_messages() {
        let replay = vec![json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "old", "update": {"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "hi"}}}})];
        let r = started(FakeConfig { load_script: replay, ..Default::default() }).await;
        let opened = r.assistant.load_conversation("s1", Some("saved-1"), "old", options()).await.unwrap();
        assert_eq!(opened.acp_session_id, "old");
        let heard = r.sink.events().into_iter().filter(|e| matches!(e, AssistantEvent::Update { conversation_id, .. } if *conversation_id == opened.conversation_id)).count();
        assert_eq!(heard, 1, "the replay arrives before the load is answered, so the conversation must already be known by then");
        let params = r.host.agent().calls("session/load")[0]["params"].clone();
        assert_eq!(params["sessionId"], "old");
    }

    #[tokio::test]
    async fn auth_required_sets_needs_sign_in() {
        let r = started(FakeConfig { auth_required: true, ..Default::default() }).await;
        let err = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.err().unwrap();
        assert!(err.contains("Sign in"), "{err}");
        assert_eq!(
            r.assistant.status(),
            AgentStatus::NeedsSignIn { methods: vec!["Log in with Claude".into()], command: Some(launch::login_command()) }
        );
        assert!(r.assistant.conversations.all().is_empty());
    }

    #[tokio::test]
    async fn an_agent_of_the_persons_own_gets_no_login_command() {
        let r = rig(FakeConfig { auth_required: true, ..Default::default() }).await;
        assert_eq!(r.assistant.start(Some("/opt/agent".into())).await, AgentStatus::Ready);
        r.assistant.new_conversation("s1", Some("saved-1"), options()).await.err().unwrap();
        assert_eq!(r.assistant.status(), AgentStatus::NeedsSignIn { methods: vec!["Log in with Claude".into()], command: None });
    }

    #[tokio::test]
    async fn a_prompt_goes_to_the_agent_and_cancel_to_it_too() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        assert_eq!(r.assistant.prompt(&opened.conversation_id, "hello").await.unwrap(), "end_turn");
        assert_eq!(r.host.agent().calls("session/prompt")[0]["params"]["sessionId"], "acp-1");
        r.assistant.cancel(&opened.conversation_id).await;
        for _ in 0..100 {
            if !r.host.agent().calls("session/cancel").is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(r.host.agent().calls("session/cancel").len(), 1);
        assert!(r.assistant.prompt("nope", "x").await.is_err());
    }

    #[tokio::test]
    async fn closing_a_session_declines_pending_approvals() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        let conversation = r.assistant.conversations.get(&opened.conversation_id).unwrap();
        let token = conversation.token.clone();
        // A query waiting for the person
        let (tools, conversation_for_call) = (Arc::clone(&r.assistant.tools), Arc::clone(&conversation));
        let call = tokio::spawn(async move { tools.call(&conversation_for_call, "run_query", json!({"sql": "SELECT 1"})).await });
        for _ in 0..200 {
            if !r.sink.approvals().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(r.sink.approvals().len(), 1);
        r.assistant.close_for_session("s1").await;
        let output = tokio::time::timeout(Duration::from_secs(2), call).await.expect("the call must not hang").unwrap();
        assert!(output.is_error);
        assert!(r.assistant.conversations.by_token(&token).is_none());
        for _ in 0..100 {
            if !r.host.agent().calls("session/cancel").is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(r.host.agent().calls("session/cancel").len(), 1);
    }

    #[tokio::test]
    async fn forget_saved_removes_the_folder() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        let folder = r.data.join("assistant/saved-1");
        assert!(folder.is_dir());
        r.assistant.close_conversation(&opened.conversation_id).await;
        assert!(folder.is_dir(), "closing a saved connection's conversation keeps its folder");
        r.assistant.forget_saved("saved-1");
        assert!(!folder.exists());
        // A hostile id touches nothing
        std::fs::create_dir_all(r.data.join("assistant/keep")).unwrap();
        r.assistant.forget_saved("../assistant");
        assert!(r.data.join("assistant/keep").is_dir());
    }

    #[tokio::test]
    async fn restart_brings_the_agent_back() {
        let r = started(FakeConfig::default()).await;
        r.assistant.shutdown().await;
        assert_eq!(r.assistant.restart(None).await, AgentStatus::Ready);
        assert_eq!(r.host.launched.lock().unwrap().len(), 2);
        assert!(r.assistant.new_conversation("s1", Some("saved-1"), options()).await.is_ok());
    }

    #[tokio::test]
    async fn signing_out_ends_the_conversations_and_asks_to_sign_in() {
        let r = started(FakeConfig::default()).await;
        let opened = r.assistant.new_conversation("s1", Some("saved-1"), options()).await.unwrap();
        r.assistant.sign_out(None).await.unwrap();
        assert_eq!(r.host.agent().calls("logout").len(), 1);
        assert!(r.assistant.conversations.all().is_empty());
        assert!(r.assistant.prompt(&opened.conversation_id, "hi").await.is_err());
        assert_eq!(r.assistant.status(), AgentStatus::NeedsSignIn { methods: vec![], command: Some(launch::login_command()) });
    }

    #[tokio::test]
    async fn signing_out_starts_the_agent_if_it_is_not_up() {
        let r = rig(FakeConfig::default()).await;
        r.assistant.sign_out(None).await.unwrap();
        assert_eq!(r.host.agent().calls("logout").len(), 1);
    }

    #[tokio::test]
    async fn signing_out_says_why_when_the_agent_cannot_start() {
        let host = Arc::new(FakeHost { node: Err(None), config: StdMutex::new(FakeConfig::default()), agents: Default::default(), launched: Default::default() });
        let r = rig_with(host).await;
        let err = r.assistant.sign_out(None).await.err().unwrap();
        assert!(err.contains("Node.js"), "{err}");
    }

    #[tokio::test]
    async fn a_conversation_needs_a_running_agent_and_an_open_session() {
        let r = rig(FakeConfig::default()).await;
        assert!(r.assistant.new_conversation("s1", Some("saved-1"), options()).await.is_err());
        assert_eq!(r.assistant.start(None).await, AgentStatus::Ready);
        assert!(r.assistant.new_conversation("missing", Some("saved-1"), options()).await.is_err());
    }
}
