//! What the assistant tells the interface, as events.

use serde::Serialize;

use super::approvals::ApprovalRequest;

pub trait EventSink: Send + Sync {
    fn emit(&self, event: AssistantEvent);
}

/// A way the agent offers to sign in.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AuthMethod {
    pub id: String,
    pub name: String,
}

/// What the assistant is doing while it starts, in order.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StartStep {
    /// Asking the person's shell for its PATH, to find Node.
    Shell,
    Node,
    /// DBunny's own tool server.
    Tools,
    /// The agent: the first time it is downloaded, which takes a while.
    Agent,
}

/// Where the agent stands (§8.3 of the design).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AgentStatus {
    Idle,
    /// `detail` is the last line the agent wrote while it started (a download, say).
    Starting { step: StartStep, detail: Option<String> },
    Ready,
    NeedsNode { found: Option<String> },
    /// `command` is what to type in a terminal to sign in, when the agent is the usual one.
    NeedsSignIn { methods: Vec<AuthMethod>, command: Option<String> },
    Unsupported { message: String },
    Failed { message: String },
    Stopped,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AssistantEvent {
    Status { status: AgentStatus },
    #[serde(rename_all = "camelCase")]
    Update { conversation_id: String, update: serde_json::Value },
    Approval { request: ApprovalRequest },
}

#[cfg(test)]
pub struct MemorySink(pub std::sync::Mutex<Vec<AssistantEvent>>);

#[cfg(test)]
impl MemorySink {
    pub fn new() -> std::sync::Arc<MemorySink> {
        std::sync::Arc::new(MemorySink(Default::default()))
    }

    pub fn events(&self) -> Vec<AssistantEvent> {
        self.0.lock().unwrap().clone()
    }

    pub fn approvals(&self) -> Vec<ApprovalRequest> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                AssistantEvent::Approval { request } => Some(request),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
impl EventSink for MemorySink {
    fn emit(&self, event: AssistantEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_for_the_frontend() {
        let status = serde_json::to_value(AssistantEvent::Status { status: AgentStatus::NeedsNode { found: Some("18.1.0".into()) } }).unwrap();
        assert_eq!(status, serde_json::json!({"type": "status", "status": {"state": "needsNode", "found": "18.1.0"}}));
        let update = serde_json::to_value(AssistantEvent::Update { conversation_id: "c1".into(), update: serde_json::json!({"a": 1}) }).unwrap();
        assert_eq!(update, serde_json::json!({"type": "update", "conversationId": "c1", "update": {"a": 1}}));
        let starting = serde_json::to_value(AgentStatus::Starting { step: StartStep::Agent, detail: Some("added 3 packages".into()) }).unwrap();
        assert_eq!(starting, serde_json::json!({"state": "starting", "step": "agent", "detail": "added 3 packages"}));
        let sign_in = serde_json::to_value(AgentStatus::NeedsSignIn { methods: vec![AuthMethod { id: "login".into(), name: "Log in".into() }], command: Some("npx x".into()) }).unwrap();
        assert_eq!(sign_in, serde_json::json!({"state": "needsSignIn", "methods": [{"id": "login", "name": "Log in"}], "command": "npx x"}));
    }
}
