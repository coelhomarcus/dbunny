//! The client of the Agent Client Protocol: it runs the agent as a child process and speaks
//! JSON-RPC 2.0 with it over its stdin and stdout, one message per line.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

use super::approvals::Approvals;
use super::conversation::Conversations;
use super::events::{AgentStatus, AssistantEvent, EventSink};
use super::launch::AgentCommand;

/// How long the agent has to answer `initialize`: the first run may download the adapter.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(120);
/// The JSON-RPC codes of the errors the client tells apart.
const AUTH_REQUIRED: i64 = -32000;
const RESOURCE_NOT_FOUND: i64 = -32002;
const METHOD_NOT_FOUND: i64 = -32601;
/// The prefix of the names the agent gives the tools of the `dbunny` MCP server.
const OWN_TOOLS: &str = "mcp__dbunny__";
/// The name of DBunny's MCP server, as agents that say the server of a call give it.
const OWN_SERVER: &str = "dbunny";
/// How long a permission request waits for the tool call it asks about to be known: some agents
/// do not name the tool in the request.
const PERMISSION_WAIT: Duration = Duration::from_millis(if cfg!(test) { 400 } else { 2000 });
pub const LOG_LINES: usize = 500;
const LIST_PAGES: usize = 10;

/// What a session needs to know to reach the database tools.
pub struct SessionSetup {
    pub cwd: PathBuf,
    pub mcp_url: String,
    pub token: String,
    /// The system prompt, for an agent that takes one per session.
    pub system_prompt: Option<String>,
    /// The `_meta` of the session, for an agent that is told what to leave out that way.
    pub meta: Option<Value>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    pub current: String,
    pub available: Vec<ModelOption>,
}

pub struct OpenedSession {
    pub acp_session: String,
    pub models: Option<ModelChoice>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub acp_session: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum AgentError {
    /// The agent wants the person to sign in; these are the ways it offers.
    AuthRequired(Vec<String>),
    NotFound,
    Stopped,
    Other(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::AuthRequired(_) => write!(f, "Sign in to Claude to use the assistant"),
            AgentError::NotFound => write!(f, "This conversation is no longer available"),
            AgentError::Stopped => write!(f, "The assistant stopped"),
            AgentError::Other(message) => write!(f, "{message}"),
        }
    }
}

/// The last lines the agent wrote to its stderr.
pub struct LogBuffer {
    lines: Mutex<VecDeque<String>>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> LogBuffer {
        LogBuffer { lines: Mutex::new(VecDeque::new()), capacity }
    }

    pub fn push(&self, line: &str) {
        let mut lines = self.lines.lock().unwrap();
        if lines.len() == self.capacity {
            lines.pop_front();
        }
        lines.push_back(line.to_string());
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().iter().cloned().collect()
    }
}

/// What the agent is told of a session so that it only has the database's tools: none of its
/// own, none of the person's settings, hooks or other MCP servers, and no way around approvals.
pub fn session_meta() -> Value {
    json!({"claudeCode": {"options": {
        "tools": [],
        "settingSources": [],
        "strictMcpConfig": true,
        "allowDangerouslySkipPermissions": false,
        "promptSuggestions": false,
    }}})
}

#[derive(Clone, Debug)]
pub struct PermissionOption {
    pub option_id: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

#[derive(Debug, PartialEq)]
pub enum PermissionAnswer {
    Selected(String),
    Cancelled,
}

/// Says yes to the database's own tools (whose approval is DBunny's) and no to anything else.
pub fn permission_answer(own: bool, options: &[PermissionOption]) -> PermissionAnswer {
    let kinds: &[&str] = if own { &["allow_once"] } else { &["reject_once", "reject_always"] };
    kinds
        .iter()
        .find_map(|kind| options.iter().find(|option| option.kind == *kind))
        .map_or(PermissionAnswer::Cancelled, |option| PermissionAnswer::Selected(option.option_id.clone()))
}

fn is_own_name(name: Option<&str>) -> bool {
    name.is_some_and(|name| name.len() > OWN_TOOLS.len() && name.starts_with(OWN_TOOLS))
}

/// Whether a `tool_call` update is a call of the database's own tools, in the shape of any agent:
/// Claude names it `mcp__dbunny__…`, Codex says which MCP server it went to.
pub fn is_own_tool_call(update: &Value) -> bool {
    is_own_name(update["_meta"]["claudeCode"]["toolName"].as_str())
        || update["rawInput"]["server"].as_str() == Some(OWN_SERVER)
        || update["title"].as_str().is_some_and(|title| title.starts_with("mcp.dbunny."))
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value, AgentError>>>>;

/// What the connection shares with the task that reads the agent.
struct Shared {
    writer: tokio::sync::Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Pending,
    next_id: AtomicU64,
    stopped: AtomicBool,
    shutting_down: AtomicBool,
    auth_methods: OnceLock<Vec<String>>,
    /// The ids of the tool calls seen that are the database's own.
    own_calls: Mutex<std::collections::HashSet<String>>,
    /// Notifications of the agent's own extensions, kept for the tests that look at them.
    #[cfg(test)]
    extensions: Mutex<Vec<Value>>,
    events: Arc<dyn EventSink>,
    conversations: Arc<Conversations>,
    approvals: Arc<Approvals>,
}

impl Shared {
    async fn write(&self, message: &Value) -> Result<(), AgentError> {
        let mut writer = self.writer.lock().await;
        let line = format!("{message}\n");
        writer.write_all(line.as_bytes()).await.map_err(|_| AgentError::Stopped)?;
        writer.flush().await.map_err(|_| AgentError::Stopped)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, AgentError> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err(AgentError::Stopped);
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (reply, answer) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, reply);
        // The reader may have given up between the check above and the insert
        if self.stopped.load(Ordering::SeqCst) {
            self.pending.lock().unwrap().remove(&id);
            return Err(AgentError::Stopped);
        }
        if let Err(e) = self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        answer.await.unwrap_or(Err(AgentError::Stopped))
    }

    async fn notify(&self, method: &str, params: Value) {
        let _ = self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params})).await;
    }

    /// The agent is gone: nothing it was asked will be answered.
    fn agent_left(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let waiting: Vec<_> = self.pending.lock().unwrap().drain().collect();
        for (_, reply) in waiting {
            let _ = reply.send(Err(AgentError::Stopped));
        }
        for conversation in self.conversations.all() {
            self.approvals.decline_all(&conversation.id);
        }
        if !self.shutting_down.load(Ordering::SeqCst) {
            self.events.emit(AssistantEvent::Status { status: AgentStatus::Stopped });
        }
    }

    fn handle_line(self: &Arc<Self>, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else { return };
        let id = message.get("id").cloned().filter(|id| !id.is_null());
        match (message.get("method").and_then(Value::as_str), id) {
            (Some(method), Some(id)) => {
                let (shared, method, params) = (Arc::clone(self), method.to_string(), message.get("params").cloned().unwrap_or(Value::Null));
                tokio::spawn(async move {
                    let reply = if method == "session/request_permission" {
                        json!({"jsonrpc": "2.0", "id": id, "result": shared.permission_reply(&params).await})
                    } else {
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": METHOD_NOT_FOUND, "message": "Method not found"}})
                    };
                    let _ = shared.write(&reply).await;
                });
            }
            (Some("session/update"), None) => {
                let params = &message["params"];
                let update = &params["update"];
                if update["sessionUpdate"] == "tool_call" && is_own_tool_call(update) {
                    if let Some(call) = update["toolCallId"].as_str() {
                        self.own_calls.lock().unwrap().insert(call.to_string());
                    }
                }
                let conversation = params["sessionId"].as_str().and_then(|session| self.conversations.by_acp_session(session));
                if let Some(conversation) = conversation {
                    self.events.emit(AssistantEvent::Update { conversation_id: conversation.id.clone(), update: params["update"].clone() });
                }
            }
            (Some(_method), None) => {
                #[cfg(test)]
                if _method.starts_with('_') && self.extensions.lock().unwrap().len() < 500 {
                    self.extensions.lock().unwrap().push(json!({"method": _method, "params": message["params"]}));
                }
            }
            (None, Some(id)) => {
                let Some(reply) = id.as_u64().and_then(|id| self.pending.lock().unwrap().remove(&id)) else { return };
                let _ = reply.send(match message.get("error") {
                    Some(error) => Err(self.failure(error)),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                });
            }
            (None, None) => {}
        }
    }

    fn failure(&self, error: &Value) -> AgentError {
        match error["code"].as_i64() {
            Some(AUTH_REQUIRED) => AgentError::AuthRequired(self.auth_methods.get().cloned().unwrap_or_default()),
            Some(RESOURCE_NOT_FOUND) => AgentError::NotFound,
            _ => AgentError::Other(error["message"].as_str().unwrap_or("The agent reported an error").to_string()),
        }
    }
}

impl Shared {
    /// What DBunny answers when the agent asks for permission: yes to the database's own tools (whose
    /// approval is DBunny's) and no to anything else, files and a terminal included.
    async fn permission_reply(&self, params: &Value) -> Value {
        let call = params["toolCall"]["toolCallId"].as_str();
        let named = params["toolCall"]["_meta"]["claudeCode"]["toolName"].as_str();
        let known = |call: Option<&str>| call.is_some_and(|call| self.own_calls.lock().unwrap().contains(call));
        let mut own = is_own_name(named) || known(call);
        // An agent that names no tool: the request says which call, and that call may be still on its way
        if !own && named.is_none() {
            let mut waited = Duration::ZERO;
            while !own && waited < PERMISSION_WAIT {
                tokio::time::sleep(Duration::from_millis(50)).await;
                waited += Duration::from_millis(50);
                own = known(call);
            }
        }
        let options: Vec<PermissionOption> = params["options"]
            .as_array()
            .map(|options| {
                options
                    .iter()
                    .filter_map(|o| Some(PermissionOption { option_id: o["optionId"].as_str()?.to_string(), kind: o["kind"].as_str()?.to_string() }))
                    .collect()
            })
            .unwrap_or_default();
        match permission_answer(own, &options) {
            PermissionAnswer::Selected(option_id) => json!({"outcome": {"outcome": "selected", "optionId": option_id}}),
            PermissionAnswer::Cancelled => json!({"outcome": {"outcome": "cancelled"}}),
        }
    }
}

/// What the agent said it can do.
struct Capabilities {
    list_sessions: bool,
    system_prompt: bool,
}

pub struct AgentHandle {
    shared: Arc<Shared>,
    capabilities: Capabilities,
    /// The config option that holds each session's model, to change it.
    model_options: Mutex<HashMap<String, String>>,
    child: Mutex<Option<tokio::process::Child>>,
}

impl AgentHandle {
    /// Starts the agent as a child process and shakes hands with it.
    pub async fn spawn(
        command: &AgentCommand,
        path: Option<String>,
        env: &[(String, String)],
        log: Arc<LogBuffer>,
        events: Arc<dyn EventSink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    ) -> Result<Arc<AgentHandle>, AgentStatus> {
        let mut child = child_process(command, path, env).map_err(|e| AgentStatus::Failed { message: format!("Couldn't start {}: {e}", command.program) })?;
        let pid = child.id();
        let (stdout, stdin, stderr) = (child.stdout.take().unwrap(), child.stdin.take().unwrap(), child.stderr.take().unwrap());
        let keep = Arc::clone(&log);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                keep.push(&line);
            }
        });
        match AgentHandle::connect(stdout, stdin, Some(child), events, conversations, approvals).await {
            Ok(agent) => Ok(agent),
            Err(status) => {
                if let Some(pid) = pid {
                    kill_tree(pid);
                }
                let AgentStatus::Failed { message } = status else { return Err(status) };
                let hint = log.lines().into_iter().rev().find(|line| !line.trim().is_empty());
                Err(AgentStatus::Failed { message: hint.map_or(message.clone(), |hint| format!("{message} ({hint})")) })
            }
        }
    }

    /// Talks to an agent over `read` and `write` (the child's stdout and stdin).
    #[cfg(test)]
    pub async fn over(
        read: impl AsyncRead + Send + Unpin + 'static,
        write: impl AsyncWrite + Send + Unpin + 'static,
        events: Arc<dyn EventSink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    ) -> Result<Arc<AgentHandle>, AgentStatus> {
        AgentHandle::connect(read, write, None, events, conversations, approvals).await
    }

    async fn connect(
        read: impl AsyncRead + Send + Unpin + 'static,
        write: impl AsyncWrite + Send + Unpin + 'static,
        child: Option<tokio::process::Child>,
        events: Arc<dyn EventSink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    ) -> Result<Arc<AgentHandle>, AgentStatus> {
        let shared = Arc::new(Shared {
            writer: tokio::sync::Mutex::new(Box::new(write)),
            pending: Default::default(),
            next_id: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            auth_methods: OnceLock::new(),
            own_calls: Default::default(),
            #[cfg(test)]
            extensions: Default::default(),
            events,
            conversations,
            approvals,
        });
        let reader = Arc::clone(&shared);
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                reader.handle_line(&line);
            }
            reader.agent_left();
        });

        let params = json!({
            "protocolVersion": 1,
            "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
            "clientInfo": {"name": "DBunny", "version": env!("CARGO_PKG_VERSION")},
        });
        let hello = tokio::time::timeout(INITIALIZE_TIMEOUT, shared.request("initialize", params)).await;
        let hello = match hello {
            Ok(Ok(hello)) => hello,
            Ok(Err(e)) => return Err(AgentStatus::Failed { message: format!("The agent didn't start: {e}") }),
            Err(_) => return Err(AgentStatus::Failed { message: "The agent didn't answer in time".into() }),
        };
        let abilities = &hello["agentCapabilities"];
        if abilities["mcpCapabilities"]["http"] != true || abilities["loadSession"] != true {
            return Err(AgentStatus::Unsupported { message: "This agent can't use DBunny's tools.".into() });
        }
        let methods = hello["authMethods"].as_array().map(|m| m.iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        let _ = shared.auth_methods.set(methods);
        let sessions = &abilities["sessionCapabilities"];
        Ok(Arc::new(AgentHandle {
            shared,
            capabilities: Capabilities { list_sessions: !sessions["list"].is_null(), system_prompt: !sessions["systemPrompt"].is_null() },
            model_options: Default::default(),
            child: Mutex::new(child),
        }))
    }

    #[cfg(test)]
    pub fn supports_system_prompt(&self) -> bool {
        self.capabilities.system_prompt
    }

    fn session_params(&self, setup: &SessionSetup) -> serde_json::Map<String, Value> {
        let mut params = serde_json::Map::new();
        params.insert("cwd".into(), json!(setup.cwd));
        params.insert(
            "mcpServers".into(),
            json!([{"type": "http", "name": "dbunny", "url": setup.mcp_url, "headers": [{"name": "Authorization", "value": format!("Bearer {}", setup.token)}]}]),
        );
        let mut meta = setup.meta.clone();
        if let Some(prompt) = &setup.system_prompt {
            if self.capabilities.system_prompt {
                params.insert("systemPrompt".into(), json!({"mode": "override", "content": [{"type": "text", "text": prompt}]}));
            } else {
                // The adapter before that capability reads a string here as the whole system prompt
                meta.get_or_insert_with(|| json!({}))["systemPrompt"] = json!(prompt);
            }
        }
        if let Some(meta) = meta {
            params.insert("_meta".into(), meta);
        }
        params
    }

    pub async fn new_session(&self, setup: &SessionSetup) -> Result<OpenedSession, AgentError> {
        let result = self.shared.request("session/new", Value::Object(self.session_params(setup))).await?;
        let acp_session = result["sessionId"].as_str().ok_or_else(|| AgentError::Other("The agent gave no session".into()))?.to_string();
        Ok(self.opened(acp_session, &result))
    }

    pub async fn load_session(&self, acp_session: &str, setup: &SessionSetup) -> Result<OpenedSession, AgentError> {
        let mut params = self.session_params(setup);
        params.insert("sessionId".into(), json!(acp_session));
        let result = self.shared.request("session/load", Value::Object(params)).await?;
        Ok(self.opened(acp_session.to_string(), &result))
    }

    fn opened(&self, acp_session: String, result: &Value) -> OpenedSession {
        let models = model_choice(result);
        if let Some((config_id, _)) = &models {
            self.model_options.lock().unwrap().insert(acp_session.clone(), config_id.clone());
        }
        OpenedSession { acp_session, models: models.map(|(_, choice)| choice) }
    }

    pub async fn list_sessions(&self, cwd: &Path) -> Result<Vec<SessionSummary>, AgentError> {
        if !self.capabilities.list_sessions {
            return Ok(vec![]);
        }
        let mut found = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..LIST_PAGES {
            let mut params = json!({"cwd": cwd});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let page = self.shared.request("session/list", params).await?;
            for session in page["sessions"].as_array().into_iter().flatten() {
                if let Some(id) = session["sessionId"].as_str() {
                    found.push(SessionSummary {
                        acp_session: id.to_string(),
                        title: session["title"].as_str().map(str::to_string),
                        updated_at: session["updatedAt"].as_str().map(str::to_string),
                    });
                }
            }
            match page["nextCursor"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(found)
    }

    /// Sends a message and waits for the turn to end; gives its stop reason.
    pub async fn prompt(&self, acp_session: &str, text: &str) -> Result<String, AgentError> {
        let result = self.shared.request("session/prompt", json!({"sessionId": acp_session, "prompt": [{"type": "text", "text": text}]})).await?;
        Ok(result["stopReason"].as_str().unwrap_or("end_turn").to_string())
    }

    pub async fn cancel(&self, acp_session: &str) {
        self.shared.notify("session/cancel", json!({"sessionId": acp_session})).await;
    }

    pub async fn set_model(&self, acp_session: &str, model_id: &str) -> Result<(), AgentError> {
        let config_id = self.model_options.lock().unwrap().get(acp_session).cloned().ok_or_else(|| AgentError::Other("This conversation has no model to change".into()))?;
        self.shared.request("session/set_config_option", json!({"sessionId": acp_session, "configId": config_id, "value": model_id})).await?;
        Ok(())
    }

    /// Signs out of the account the agent uses. The agent's own credentials are what go, so every other
    /// program that uses them is signed out too.
    pub async fn logout(&self) -> Result<(), AgentError> {
        self.shared.request("logout", json!({})).await?;
        Ok(())
    }

    /// The notifications of the agent's own extensions received so far.
    #[cfg(test)]
    pub(crate) fn extension_messages(&self) -> Vec<Value> {
        self.shared.extensions.lock().unwrap().clone()
    }

    /// Ends the agent without telling the interface that it stopped.
    pub async fn shutdown(&self) {
        self.shared.shutting_down.store(true, Ordering::SeqCst);
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            if let Some(pid) = child.id() {
                kill_tree(pid);
            }
            let _ = child.kill().await;
        }
    }
}

/// The command as a child process of its own: its output piped, no console window on Windows, and
/// on unix a process group of its own, so that what it starts can be ended with it.
fn child_process(command: &AgentCommand, path: Option<String>, env: &[(String, String)]) -> std::io::Result<tokio::process::Child> {
    let mut process = tokio::process::Command::new(&command.program);
    process.args(&command.args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    if let Some(path) = path {
        process.env("PATH", path);
    }
    // The profile's variables win over what the app inherited, a CODEX_CONFIG of the person's included
    process.envs(env.iter().cloned());
    #[cfg(unix)]
    process.process_group(0);
    #[cfg(windows)]
    process.creation_flags(super::launch::CREATE_NO_WINDOW);
    process.spawn()
}

/// Ends the process `pid` and everything it started. `npx` runs the agent as a child of its own,
/// which would otherwise outlive it.
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    // SAFETY: killpg only sends a signal; the group is the one `child_process` made for this pid
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(super::launch::CREATE_NO_WINDOW)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// The config option that holds the model, as `(its id, the choice)`.
fn model_choice(result: &Value) -> Option<(String, ModelChoice)> {
    let option = result["configOptions"].as_array()?.iter().find(|o| o["category"] == "model" || o["id"] == "model")?;
    let mut available = Vec::new();
    for entry in option["options"].as_array()? {
        // Options come flat, or in groups
        for item in entry["options"].as_array().map(|group| group.iter().collect::<Vec<_>>()).unwrap_or_else(|| vec![entry]) {
            if let Some(id) = item["value"].as_str() {
                available.push(ModelOption { id: id.to_string(), name: item["name"].as_str().unwrap_or(id).to_string() });
            }
        }
    }
    Some((option["id"].as_str()?.to_string(), ModelChoice { current: option["currentValue"].as_str()?.to_string(), available }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::assistant::conversation::ConversationOptions;
    use crate::assistant::events::{AssistantEvent, MemorySink};
    use crate::assistant::fake_agent::{FakeAgent, FakeConfig};

    struct Rig {
        agent: Arc<AgentHandle>,
        fake: FakeAgent,
        sink: Arc<MemorySink>,
        conversations: Arc<Conversations>,
        approvals: Arc<Approvals>,
    }

    async fn rig(config: FakeConfig) -> Result<Rig, AgentStatus> {
        let (read, write, fake) = FakeAgent::start(config);
        let (sink, conversations, approvals) = (MemorySink::new(), Arc::new(Conversations::default()), Arc::new(Approvals::default()));
        let events: Arc<dyn EventSink> = sink.clone();
        let agent = AgentHandle::over(read, write, events, Arc::clone(&conversations), Arc::clone(&approvals)).await?;
        Ok(Rig { agent, fake, sink, conversations, approvals })
    }

    fn setup() -> SessionSetup {
        SessionSetup { cwd: "/tmp/work".into(), mcp_url: "http://127.0.0.1:4000/mcp".into(), token: "tok".into(), system_prompt: Some("You are the assistant.".into()), meta: Some(session_meta()) }
    }

    fn options() -> ConversationOptions {
        ConversationOptions { ask_before_queries: true, schema_only: false, row_limit: 100, instructions: String::new() }
    }

    fn update(session: &str, kind: &str) -> Value {
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session, "update": {"sessionUpdate": kind, "content": {"type": "text", "text": "hi"}}}})
    }

    fn permission_request(id: u64, tool: Option<&str>) -> Value {
        let call = match tool {
            Some(name) => json!({"toolCallId": "t", "_meta": {"claudeCode": {"toolName": name}}}),
            None => json!({"toolCallId": "t"}),
        };
        json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {
            "sessionId": "acp-1", "toolCall": call,
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}, {"optionId": "reject", "name": "Reject", "kind": "reject_once"}],
        }})
    }

    fn answer_to(rig: &Rig, id: u64) -> Value {
        rig.fake.answers.lock().unwrap().iter().find(|a| a["id"] == id).cloned().unwrap_or(Value::Null)
    }

    #[tokio::test]
    async fn initialize_declines_fs_and_terminal() {
        let r = rig(FakeConfig::default()).await.unwrap();
        let params = r.fake.calls("initialize")[0]["params"].clone();
        assert_eq!(params["protocolVersion"], 1);
        assert_eq!(params["clientCapabilities"]["fs"], json!({"readTextFile": false, "writeTextFile": false}));
        assert_eq!(params["clientCapabilities"]["terminal"], false);
    }

    #[tokio::test]
    async fn agent_without_http_mcp_is_unsupported() {
        let status = rig(FakeConfig { http_mcp: false, ..Default::default() }).await.err().unwrap();
        assert_eq!(status, AgentStatus::Unsupported { message: "This agent can't use DBunny's tools.".into() });
        let status = rig(FakeConfig { load_session: false, ..Default::default() }).await.err().unwrap();
        assert!(matches!(status, AgentStatus::Unsupported { .. }));
    }

    #[tokio::test]
    async fn a_session_without_system_prompt_or_meta_carries_neither() {
        let r = rig(FakeConfig::default()).await.unwrap();
        r.agent.new_session(&SessionSetup { system_prompt: None, meta: None, ..setup() }).await.unwrap();
        let params = r.fake.calls("session/new")[0]["params"].clone();
        assert!(params.get("systemPrompt").is_none() && params.get("_meta").is_none(), "{params}");
        // The agent that reads the prompt from `_meta` still gets only the prompt there
        let without = rig(FakeConfig { system_prompt: false, ..Default::default() }).await.unwrap();
        without.agent.new_session(&SessionSetup { meta: None, ..setup() }).await.unwrap();
        let params = without.fake.calls("session/new")[0]["params"].clone();
        assert_eq!(params["_meta"], json!({"systemPrompt": "You are the assistant."}));
    }

    #[tokio::test]
    async fn new_session_sends_cwd_mcp_server_and_meta() {
        let r = rig(FakeConfig::default()).await.unwrap();
        let opened = r.agent.new_session(&setup()).await.unwrap();
        assert_eq!(opened.acp_session, "acp-1");
        let models = opened.models.unwrap();
        assert_eq!((models.current.as_str(), models.available.len()), ("default", 2));
        let params = r.fake.calls("session/new")[0]["params"].clone();
        assert_eq!(params["cwd"], "/tmp/work");
        assert_eq!(
            params["mcpServers"],
            json!([{"type": "http", "name": "dbunny", "url": "http://127.0.0.1:4000/mcp", "headers": [{"name": "Authorization", "value": "Bearer tok"}]}])
        );
        assert_eq!(params["_meta"], session_meta());
        assert_eq!(params["systemPrompt"], json!({"mode": "override", "content": [{"type": "text", "text": "You are the assistant."}]}));
        assert!(r.agent.supports_system_prompt());

        let without = rig(FakeConfig { system_prompt: false, ..Default::default() }).await.unwrap();
        without.agent.new_session(&setup()).await.unwrap();
        let without_params = without.fake.calls("session/new")[0]["params"].clone();
        assert!(without_params.get("systemPrompt").is_none());
        // The Claude adapter without that capability takes the prompt in `_meta`
        assert_eq!(without_params["_meta"]["systemPrompt"], "You are the assistant.");
        assert_eq!(without_params["_meta"]["claudeCode"], session_meta()["claudeCode"]);
        assert!(!without.agent.supports_system_prompt());
    }

    #[test]
    fn session_meta_leaves_the_agent_only_dbunnys_tools() {
        let options = &session_meta()["claudeCode"]["options"];
        assert_eq!(options["tools"], json!([]));
        assert_eq!(options["settingSources"], json!([]));
        assert_eq!(options["strictMcpConfig"], true);
        assert_eq!(options["allowDangerouslySkipPermissions"], false);
        assert_eq!(options["promptSuggestions"], false);
    }

    #[tokio::test]
    async fn load_session_sends_a_new_token() {
        let r = rig(FakeConfig::default()).await.unwrap();
        let mut again = setup();
        again.token = "tok2".into();
        r.agent.load_session("acp-9", &again).await.unwrap();
        let params = r.fake.calls("session/load")[0]["params"].clone();
        assert_eq!(params["sessionId"], "acp-9");
        assert_eq!(params["cwd"], "/tmp/work");
        assert_eq!(params["mcpServers"][0]["headers"][0]["value"], "Bearer tok2");
    }

    #[tokio::test]
    async fn list_sessions_reads_title_and_date() {
        let sessions = vec![json!({"sessionId": "a", "cwd": "/tmp/work", "title": "Sales", "updatedAt": "2026-10-19T10:00:00Z"}), json!({"sessionId": "b", "cwd": "/tmp/work"})];
        let r = rig(FakeConfig { sessions, ..Default::default() }).await.unwrap();
        let listed = r.agent.list_sessions(std::path::Path::new("/tmp/work")).await.unwrap();
        assert_eq!(
            listed,
            vec![
                SessionSummary { acp_session: "a".into(), title: Some("Sales".into()), updated_at: Some("2026-10-19T10:00:00Z".into()) },
                SessionSummary { acp_session: "b".into(), title: None, updated_at: None },
            ]
        );
        assert_eq!(r.fake.calls("session/list")[0]["params"]["cwd"], "/tmp/work");
        let none = rig(FakeConfig { list_sessions: false, ..Default::default() }).await.unwrap();
        assert!(none.agent.list_sessions(std::path::Path::new("/tmp/work")).await.unwrap().is_empty());
        assert!(none.fake.calls("session/list").is_empty());
    }

    #[tokio::test]
    async fn logout_asks_the_agent_to_sign_out() {
        let r = rig(FakeConfig::default()).await.unwrap();
        r.agent.logout().await.unwrap();
        assert_eq!(r.fake.calls("logout").len(), 1);
        assert_eq!(r.fake.calls("logout")[0]["params"], json!({}));
    }

    #[tokio::test]
    async fn updates_reach_the_conversation() {
        let script = vec![update("acp-1", "agent_message_chunk"), update("someone-else", "agent_message_chunk")];
        let r = rig(FakeConfig { prompt_script: script, ..Default::default() }).await.unwrap();
        let conversation = r.conversations.create("s1", options());
        let opened = r.agent.new_session(&setup()).await.unwrap();
        *conversation.acp_session.lock().unwrap() = Some(opened.acp_session.clone());
        assert_eq!(r.agent.prompt("acp-1", "hello").await.unwrap(), "end_turn");
        let updates: Vec<(String, Value)> = r
            .sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                AssistantEvent::Update { conversation_id, update } => Some((conversation_id, update)),
                _ => None,
            })
            .collect();
        assert_eq!(updates.len(), 1, "{updates:?}");
        assert_eq!(updates[0].0, conversation.id);
        assert_eq!(updates[0].1["sessionUpdate"], "agent_message_chunk");
        let prompt = r.fake.calls("session/prompt")[0]["params"].clone();
        assert_eq!(prompt, json!({"sessionId": "acp-1", "prompt": [{"type": "text", "text": "hello"}]}));
    }

    #[tokio::test]
    async fn dbunny_tools_are_allowed_and_others_rejected() {
        let script = vec![
            permission_request(100, Some("mcp__dbunny__run_query")),
            permission_request(101, Some("Bash")),
            permission_request(102, Some("mcp__other__x")),
        ];
        let r = rig(FakeConfig { prompt_script: script, ..Default::default() }).await.unwrap();
        r.agent.new_session(&setup()).await.unwrap();
        r.agent.prompt("acp-1", "go").await.unwrap();
        assert_eq!(answer_to(&r, 100)["result"]["outcome"], json!({"outcome": "selected", "optionId": "allow"}));
        assert_eq!(answer_to(&r, 101)["result"]["outcome"], json!({"outcome": "selected", "optionId": "reject"}));
        assert_eq!(answer_to(&r, 102)["result"]["outcome"], json!({"outcome": "selected", "optionId": "reject"}));
    }

    #[test]
    fn permission_goes_to_the_database_tools_only() {
        let options = vec![
            PermissionOption { option_id: "allow".into(), kind: "allow_once".into() },
            PermissionOption { option_id: "reject".into(), kind: "reject_once".into() },
        ];
        assert_eq!(permission_answer(true, &options), PermissionAnswer::Selected("allow".into()));
        assert_eq!(permission_answer(false, &options), PermissionAnswer::Selected("reject".into()));
        // Without an option of the right kind there is nothing to select
        let only_allow = vec![PermissionOption { option_id: "a".into(), kind: "allow_always".into() }];
        assert_eq!(permission_answer(false, &only_allow), PermissionAnswer::Cancelled);
    }

    #[test]
    fn database_tool_calls_are_told_in_the_shape_of_any_agent() {
        for update in [
            json!({"sessionUpdate": "tool_call", "_meta": {"claudeCode": {"toolName": "mcp__dbunny__run_query"}}}),
            json!({"sessionUpdate": "tool_call", "title": "mcp.dbunny.run_query", "rawInput": {"server": "dbunny", "tool": "run_query", "arguments": {}}}),
            json!({"sessionUpdate": "tool_call", "rawInput": {"server": "dbunny"}}),
            json!({"sessionUpdate": "tool_call", "title": "mcp.dbunny.list_tables"}),
        ] {
            assert!(is_own_tool_call(&update), "{update}");
        }
        for update in [
            json!({"_meta": {"claudeCode": {"toolName": "Bash"}}}),
            json!({"_meta": {"claudeCode": {"toolName": "mcp__dbunny__"}}}),
            json!({"_meta": {"claudeCode": {"toolName": "mcp__dbunny_evil__x"}}}),
            json!({"rawInput": {"server": "other", "tool": "x"}}),
            json!({"title": "Run command?"}),
            json!({"title": "mcp.dbunnyx.run_query"}),
            json!({}),
        ] {
            assert!(!is_own_tool_call(&update), "{update}");
        }
    }

    fn codex_permission(id: u64, call: &str) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {
            "sessionId": "acp-1", "toolCall": {"toolCallId": call, "kind": "execute", "status": "pending"}, "_meta": {"is_mcp_tool_approval": true},
            "options": [{"optionId": "allow_once", "name": "Allow", "kind": "allow_once"}, {"optionId": "allow_always", "name": "Always", "kind": "allow_always"}, {"optionId": "cancel", "name": "Cancel", "kind": "reject_once"}],
        }})
    }

    fn codex_tool_call(call: &str, title: &str, server: &str) -> Value {
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "acp-1", "update": {
            "sessionUpdate": "tool_call", "toolCallId": call, "kind": "execute", "title": title, "status": "in_progress",
            "rawInput": {"server": server, "tool": "run_query", "arguments": {}},
        }}})
    }

    #[tokio::test]
    async fn a_permission_without_a_name_is_decided_by_its_tool_call() {
        let script = vec![
            codex_tool_call("own", "mcp.dbunny.run_query", "dbunny"),
            codex_permission(300, "own"),
            codex_tool_call("other", "mcp.github.create_issue", "github"),
            codex_permission(301, "other"),
            codex_permission(302, "never-seen"),
        ];
        let r = rig(FakeConfig { prompt_script: script, ..Default::default() }).await.unwrap();
        r.agent.new_session(&setup()).await.unwrap();
        r.agent.prompt("acp-1", "go").await.unwrap();
        assert_eq!(answer_to(&r, 300)["result"]["outcome"], json!({"outcome": "selected", "optionId": "allow_once"}));
        for id in [301, 302] {
            assert_eq!(answer_to(&r, id)["result"]["outcome"], json!({"outcome": "selected", "optionId": "cancel"}), "{id}");
        }
    }

    #[tokio::test]
    async fn a_permission_that_comes_before_its_tool_call_waits_a_moment() {
        let script = vec![
            codex_permission(310, "late"),
            json!({"_delay_ms": 100}),
            codex_tool_call("late", "mcp.dbunny.run_query", "dbunny"),
        ];
        let r = rig(FakeConfig { prompt_script: script, ..Default::default() }).await.unwrap();
        r.agent.new_session(&setup()).await.unwrap();
        r.agent.prompt("acp-1", "go").await.unwrap();
        assert_eq!(answer_to(&r, 310)["result"]["outcome"], json!({"outcome": "selected", "optionId": "allow_once"}));
    }

    #[tokio::test]
    async fn fs_and_terminal_requests_fail() {
        let script = vec![
            json!({"jsonrpc": "2.0", "id": 200, "method": "fs/read_text_file", "params": {"sessionId": "acp-1", "path": "/etc/passwd"}}),
            json!({"jsonrpc": "2.0", "id": 201, "method": "terminal/create", "params": {"sessionId": "acp-1", "command": "ls"}}),
        ];
        let r = rig(FakeConfig { prompt_script: script, ..Default::default() }).await.unwrap();
        r.agent.new_session(&setup()).await.unwrap();
        r.agent.prompt("acp-1", "go").await.unwrap();
        for id in [200, 201] {
            assert_eq!(answer_to(&r, id)["error"]["code"], -32601, "{id}");
        }
    }

    #[tokio::test]
    async fn auth_required_maps_to_sign_in() {
        let r = rig(FakeConfig { auth_required: true, ..Default::default() }).await.unwrap();
        let err = r.agent.new_session(&setup()).await.err().unwrap();
        assert_eq!(err, AgentError::AuthRequired(vec!["Log in with Claude".into()]));
        let err = r.agent.load_session("acp-1", &setup()).await.err().unwrap();
        assert!(matches!(err, AgentError::AuthRequired(_)));
    }

    #[tokio::test]
    async fn cancel_reaches_the_agent() {
        let r = rig(FakeConfig::default()).await.unwrap();
        r.agent.cancel("acp-1").await;
        for _ in 0..100 {
            if !r.fake.calls("session/cancel").is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let cancels = r.fake.calls("session/cancel");
        assert_eq!(cancels.len(), 1);
        assert_eq!(cancels[0]["params"]["sessionId"], "acp-1");
        assert!(cancels[0].get("id").is_none());
    }

    #[tokio::test]
    async fn set_model_sends_the_config_option() {
        let r = rig(FakeConfig::default()).await.unwrap();
        r.agent.new_session(&setup()).await.unwrap();
        r.agent.set_model("acp-1", "haiku").await.unwrap();
        let call = r.fake.calls("session/set_config_option")[0]["params"].clone();
        assert_eq!(call, json!({"sessionId": "acp-1", "configId": "model", "value": "haiku"}));
    }

    #[tokio::test]
    async fn agent_exit_mid_turn_reports_stopped() {
        let r = rig(FakeConfig { die_on_prompt: true, ..Default::default() }).await.unwrap();
        let conversation = r.conversations.create("s1", options());
        let opened = r.agent.new_session(&setup()).await.unwrap();
        *conversation.acp_session.lock().unwrap() = Some(opened.acp_session);
        // A query waiting for approval when the agent goes
        let (approvals, sink, id) = (Arc::clone(&r.approvals), Arc::clone(&r.sink), conversation.id.clone());
        let waiting = tokio::spawn(async move {
            let request = crate::assistant::approvals::ApprovalRequest { id: "a1".into(), conversation_id: id, tool: "run_query".into(), sql: "SELECT 1".into(), analyze: false };
            approvals.ask(request, sink.as_ref()).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(r.agent.prompt("acp-1", "go").await.err().unwrap(), AgentError::Stopped);
        assert_eq!(waiting.await.unwrap(), crate::assistant::approvals::Decision::Decline);
        assert!(r.sink.events().iter().any(|e| matches!(e, AssistantEvent::Status { status: AgentStatus::Stopped })));
        // And nothing more can be asked
        assert_eq!(r.agent.new_session(&setup()).await.err().unwrap(), AgentError::Stopped);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_profile_environment_wins_over_the_inherited_one() {
        use tokio::io::AsyncBufReadExt;
        std::env::set_var("DBUNNY_TEST_PROFILE_VAR", "inherited");
        let command = AgentCommand { program: "sh".into(), args: vec!["-c".into(), "echo $DBUNNY_TEST_PROFILE_VAR".into()] };
        let mut child = child_process(&command, None, &[("DBUNNY_TEST_PROFILE_VAR".into(), "profile".into())]).unwrap();
        let line = BufReader::new(child.stdout.take().unwrap()).lines().next_line().await.unwrap().unwrap();
        assert_eq!(line, "profile");
        let _ = child.wait().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ending_the_agent_ends_what_it_started() {
        use tokio::io::AsyncBufReadExt;
        // Like npx: the command starts the real thing as a child of its own and waits for it
        let command = AgentCommand { program: "sh".into(), args: vec!["-c".into(), "sleep 300 & echo $!; wait".into()] };
        let mut child = child_process(&command, None, &[]).unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let grandchild: i32 = lines.next_line().await.unwrap().unwrap().trim().parse().unwrap();
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(grandchild));
        kill_tree(child.id().unwrap());
        let _ = child.wait().await;
        for _ in 0..50 {
            if !alive(grandchild) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!alive(grandchild), "the child of the command is still running");
    }

    #[test]
    fn the_log_keeps_the_last_lines() {
        let log = LogBuffer::new(500);
        for i in 1..=600 {
            log.push(&format!("line {i}"));
        }
        let lines = log.lines();
        assert_eq!(lines.len(), 500);
        assert_eq!((lines[0].as_str(), lines[499].as_str()), ("line 101", "line 600"));
    }
}
