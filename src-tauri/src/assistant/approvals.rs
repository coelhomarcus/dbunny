//! Queries waiting for the person's say, between the tool that asks and the interface that answers.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::events::{AssistantEvent, EventSink};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub conversation_id: String,
    /// `run_query` or `explain_query`.
    pub tool: String,
    pub sql: String,
    pub analyze: bool,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Once,
    ReadsInChat,
    Decline,
}

struct Pending {
    conversation_id: String,
    reply: oneshot::Sender<Decision>,
}

#[derive(Default)]
pub struct Approvals {
    pending: Mutex<HashMap<String, Pending>>,
}

impl Approvals {
    /// Tells the interface and waits for the answer. A request nobody answers before it is
    /// dropped counts as a decline.
    pub async fn ask(&self, request: ApprovalRequest, events: &dyn EventSink) -> Decision {
        let (reply, answer) = oneshot::channel();
        self.pending.lock().unwrap().insert(request.id.clone(), Pending { conversation_id: request.conversation_id.clone(), reply });
        events.emit(AssistantEvent::Approval { request });
        answer.await.unwrap_or(Decision::Decline)
    }

    /// Answers a waiting request; false when there is none with that id.
    pub fn answer(&self, id: &str, decision: Decision) -> bool {
        match self.pending.lock().unwrap().remove(id) {
            Some(pending) => pending.reply.send(decision).is_ok(),
            None => false,
        }
    }

    /// Declines everything the conversation is waiting on.
    pub fn decline_all(&self, conversation_id: &str) {
        let mut pending = self.pending.lock().unwrap();
        let ids: Vec<String> = pending.iter().filter(|(_, p)| p.conversation_id == conversation_id).map(|(id, _)| id.clone()).collect();
        for id in ids {
            if let Some(p) = pending.remove(&id) {
                let _ = p.reply.send(Decision::Decline);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::assistant::events::MemorySink;

    fn request(id: &str, conversation: &str) -> ApprovalRequest {
        ApprovalRequest { id: id.into(), conversation_id: conversation.into(), tool: "run_query".into(), sql: "SELECT 1".into(), analyze: false }
    }

    async fn waiting(approvals: &Arc<Approvals>, sink: &Arc<MemorySink>, id: &str, conversation: &str) -> tokio::task::JoinHandle<Decision> {
        let (task_approvals, task_sink) = (Arc::clone(approvals), Arc::clone(sink));
        let request = request(id, conversation);
        let handle = tokio::spawn(async move { task_approvals.ask(request, task_sink.as_ref()).await });
        // Let it register
        for _ in 0..50 {
            if sink.approvals().iter().any(|r| r.id == id) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        handle
    }

    #[tokio::test]
    async fn answer_wakes_the_waiting_ask() {
        let (approvals, sink) = (Arc::new(Approvals::default()), MemorySink::new());
        let ask = waiting(&approvals, &sink, "a1", "c1").await;
        assert!(approvals.answer("a1", Decision::ReadsInChat));
        assert_eq!(ask.await.unwrap(), Decision::ReadsInChat);
    }

    #[tokio::test]
    async fn answer_of_an_unknown_id_is_false() {
        assert!(!Approvals::default().answer("nope", Decision::Once));
    }

    #[tokio::test]
    async fn decline_all_answers_decline() {
        let (approvals, sink) = (Arc::new(Approvals::default()), MemorySink::new());
        let (one, two, other) = (waiting(&approvals, &sink, "a1", "c1").await, waiting(&approvals, &sink, "a2", "c1").await, waiting(&approvals, &sink, "a3", "c2").await);
        approvals.decline_all("c1");
        assert_eq!((one.await.unwrap(), two.await.unwrap()), (Decision::Decline, Decision::Decline));
        assert!(!other.is_finished());
        assert!(approvals.answer("a3", Decision::Once));
        assert_eq!(other.await.unwrap(), Decision::Once);
    }

    #[tokio::test]
    async fn ask_emits_an_approval_event() {
        let (approvals, sink) = (Arc::new(Approvals::default()), MemorySink::new());
        let ask = waiting(&approvals, &sink, "a1", "c1").await;
        assert_eq!(sink.approvals(), vec![request("a1", "c1")]);
        approvals.answer("a1", Decision::Once);
        ask.await.unwrap();
    }
}
