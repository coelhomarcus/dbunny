//! An agent for the tests of the ACP client: it speaks the protocol over in-memory pipes,
//! records what the client sent and plays back what a test scripts.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

#[derive(Clone)]
pub struct FakeConfig {
    pub http_mcp: bool,
    pub load_session: bool,
    pub system_prompt: bool,
    pub list_sessions: bool,
    /// `session/new` and `session/load` answer "authentication required".
    pub auth_required: bool,
    /// Sent to the client, in order, before a prompt is answered. Items with an `id` are
    /// requests: the agent waits for the client to answer them all.
    pub prompt_script: Vec<Value>,
    /// The agent goes away in the middle of a prompt.
    pub die_on_prompt: bool,
    pub sessions: Vec<Value>,
    /// Sent to the client before `session/load` is answered, like the replay of a conversation.
    pub load_script: Vec<Value>,
}

impl Default for FakeConfig {
    fn default() -> Self {
        FakeConfig {
            http_mcp: true,
            load_session: true,
            system_prompt: true,
            list_sessions: true,
            auth_required: false,
            prompt_script: vec![],
            die_on_prompt: false,
            sessions: vec![],
            load_script: vec![],
        }
    }
}

#[derive(Clone)]
pub struct FakeAgent {
    /// Every request and notification the client sent, in order.
    pub received: Arc<Mutex<Vec<Value>>>,
    /// The client's answers to the agent's own requests.
    pub answers: Arc<Mutex<Vec<Value>>>,
}

impl FakeAgent {
    /// The client's end of the pipes, and the agent running on the other end.
    pub fn start(config: FakeConfig) -> (DuplexStream, DuplexStream, FakeAgent) {
        let (client_write, agent_read) = tokio::io::duplex(1 << 20);
        let (agent_write, client_read) = tokio::io::duplex(1 << 20);
        let agent = FakeAgent { received: Default::default(), answers: Default::default() };
        tokio::spawn(run(config, agent_read, agent_write, Arc::clone(&agent.received), Arc::clone(&agent.answers)));
        (client_read, client_write, agent)
    }

    /// The requests and notifications of one method.
    pub fn calls(&self, method: &str) -> Vec<Value> {
        self.received.lock().unwrap().iter().filter(|m| m["method"] == method).cloned().collect()
    }
}

async fn send(out: &mut DuplexStream, message: &Value) {
    let _ = out.write_all(format!("{message}\n").as_bytes()).await;
}

async fn run(config: FakeConfig, input: DuplexStream, mut out: DuplexStream, received: Arc<Mutex<Vec<Value>>>, answers: Arc<Mutex<Vec<Value>>>) {
    let mut lines = BufReader::new(input).lines();
    let mut sessions = 0;
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if message.get("method").is_none() {
            answers.lock().unwrap().push(message);
            continue;
        }
        received.lock().unwrap().push(message.clone());
        let Some(id) = message.get("id").cloned() else { continue };
        let result = match message["method"].as_str().unwrap_or("") {
            "initialize" => Ok(json!({
                "protocolVersion": 1,
                "agentCapabilities": {
                    "loadSession": config.load_session,
                    "mcpCapabilities": {"http": config.http_mcp, "sse": false},
                    "sessionCapabilities": {
                        "list": if config.list_sessions { json!({}) } else { Value::Null },
                        "systemPrompt": if config.system_prompt { json!({"mode": {"override": {}}}) } else { Value::Null },
                    },
                },
                "authMethods": [{"id": "claude-login", "name": "Log in with Claude"}],
            })),
            "session/new" | "session/load" if config.auth_required => Err(json!({"code": -32000, "message": "Authentication required"})),
            "session/new" => {
                sessions += 1;
                Ok(json!({
                    "sessionId": format!("acp-{sessions}"),
                    "configOptions": [{
                        "id": "model", "name": "Model", "category": "model", "type": "select",
                        "currentValue": "default",
                        "options": [{"value": "default", "name": "Default"}, {"value": "haiku", "name": "Haiku"}],
                    }],
                }))
            }
            "session/load" => {
                for item in &config.load_script {
                    send(&mut out, item).await;
                }
                Ok(json!({}))
            }
            "session/list" => Ok(json!({"sessions": config.sessions})),
            "session/set_config_option" | "logout" => Ok(json!({})),
            "session/prompt" => {
                let before = answers.lock().unwrap().len();
                let requests = config.prompt_script.iter().filter(|m| m.get("id").is_some()).count();
                for item in &config.prompt_script {
                    send(&mut out, item).await;
                }
                for _ in 0..400 {
                    if answers.lock().unwrap().len() >= before + requests {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if config.die_on_prompt {
                    return;
                }
                Ok(json!({"stopReason": "end_turn"}))
            }
            _ => Err(json!({"code": -32601, "message": "Method not found"})),
        };
        let reply = match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        send(&mut out, &reply).await;
    }
}
