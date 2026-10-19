//! The assistant: starting the agent, talking with it and answering its requests.

use serde_json::{json, Value};
use tauri::{Emitter, State};

use crate::assistant::agent::SessionSummary;
use crate::assistant::approvals::Decision;
use crate::assistant::conversation::ConversationOptions;
use crate::assistant::events::{AgentStatus, AssistantEvent, EventSink};
use crate::assistant::{Assistant, OpenedConversation};

/// Sends the assistant's events to the window.
pub struct TauriEvents(pub tauri::AppHandle);

impl EventSink for TauriEvents {
    fn emit(&self, event: AssistantEvent) {
        let (name, payload) = emitted(&event);
        let _ = self.0.emit(name, payload);
    }
}

/// The name of the window event for `event`, and what it carries.
pub fn emitted(event: &AssistantEvent) -> (&'static str, Value) {
    match event {
        AssistantEvent::Status { status } => ("assistant:status", json!(status)),
        AssistantEvent::Update { conversation_id, update } => ("assistant:update", json!({"conversationId": conversation_id, "update": update})),
        AssistantEvent::Approval { request } => ("assistant:approval", json!(request)),
    }
}

#[tauri::command]
pub async fn assistant_status(assistant: State<'_, Assistant>) -> Result<AgentStatus, String> {
    Ok(assistant.status())
}

/// Starts the agent unless it is up; `command` is the person's own command, if any.
#[tauri::command]
pub async fn assistant_start(assistant: State<'_, Assistant>, command: Option<String>) -> Result<AgentStatus, String> {
    Ok(assistant.start(command).await)
}

#[tauri::command]
pub async fn assistant_restart(assistant: State<'_, Assistant>, command: Option<String>) -> Result<AgentStatus, String> {
    Ok(assistant.restart(command).await)
}

#[tauri::command]
pub async fn assistant_new_conversation(
    assistant: State<'_, Assistant>,
    session_id: String,
    saved_id: Option<String>,
    options: ConversationOptions,
) -> Result<OpenedConversation, String> {
    assistant.new_conversation(&session_id, saved_id.as_deref(), options).await
}

#[tauri::command]
pub async fn assistant_list_conversations(
    assistant: State<'_, Assistant>,
    session_id: String,
    saved_id: Option<String>,
) -> Result<Vec<SessionSummary>, String> {
    assistant.list_conversations(&session_id, saved_id.as_deref()).await
}

#[tauri::command]
pub async fn assistant_load_conversation(
    assistant: State<'_, Assistant>,
    session_id: String,
    saved_id: Option<String>,
    acp_session_id: String,
    options: ConversationOptions,
) -> Result<OpenedConversation, String> {
    assistant.load_conversation(&session_id, saved_id.as_deref(), &acp_session_id, options).await
}

/// Sends a message and answers when the agent's turn is over, with why it ended.
#[tauri::command]
pub async fn assistant_prompt(assistant: State<'_, Assistant>, conversation_id: String, text: String) -> Result<String, String> {
    assistant.prompt(&conversation_id, &text).await
}

#[tauri::command]
pub async fn assistant_cancel(assistant: State<'_, Assistant>, conversation_id: String) -> Result<(), String> {
    assistant.cancel(&conversation_id).await;
    Ok(())
}

#[tauri::command]
pub async fn assistant_approve(assistant: State<'_, Assistant>, approval_id: String, decision: Decision) -> Result<bool, String> {
    Ok(assistant.approve(&approval_id, decision))
}

#[tauri::command]
pub async fn assistant_set_model(assistant: State<'_, Assistant>, conversation_id: String, model_id: String) -> Result<(), String> {
    assistant.set_model(&conversation_id, &model_id).await
}

#[tauri::command]
pub async fn assistant_close_conversation(assistant: State<'_, Assistant>, conversation_id: String) -> Result<(), String> {
    assistant.close_conversation(&conversation_id).await;
    Ok(())
}

/// Signs out of Claude on this computer; every conversation ends and the panel asks to sign in again.
#[tauri::command]
pub async fn assistant_sign_out(assistant: State<'_, Assistant>, command: Option<String>) -> Result<(), String> {
    assistant.sign_out(command).await
}

#[tauri::command]
pub async fn assistant_log(assistant: State<'_, Assistant>) -> Result<Vec<String>, String> {
    Ok(assistant.log().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::approvals::ApprovalRequest;

    #[test]
    fn events_go_to_the_window_under_their_names() {
        let (name, payload) = emitted(&AssistantEvent::Status { status: AgentStatus::Ready });
        assert_eq!((name, payload), ("assistant:status", json!({"state": "ready"})));
        let update = AssistantEvent::Update { conversation_id: "c1".into(), update: json!({"sessionUpdate": "agent_message_chunk"}) };
        assert_eq!(emitted(&update), ("assistant:update", json!({"conversationId": "c1", "update": {"sessionUpdate": "agent_message_chunk"}})));
        let request = ApprovalRequest { id: "a1".into(), conversation_id: "c1".into(), tool: "run_query".into(), sql: "SELECT 1".into(), analyze: false };
        let (name, payload) = emitted(&AssistantEvent::Approval { request });
        assert_eq!(name, "assistant:approval");
        assert_eq!(payload, json!({"id": "a1", "conversationId": "c1", "tool": "run_query", "sql": "SELECT 1", "analyze": false}));
    }
}
