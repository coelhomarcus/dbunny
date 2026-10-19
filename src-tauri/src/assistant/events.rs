//! What the assistant tells the interface, as events.

use serde::Serialize;

use super::approvals::ApprovalRequest;

pub trait EventSink: Send + Sync {
    fn emit(&self, event: AssistantEvent);
}

/// Where the agent stands (§8.3 of the design).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AgentStatus {
    Idle,
    Starting,
    Ready,
    NeedsNode { found: Option<String> },
    NeedsSignIn { methods: Vec<String> },
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
    }
}
