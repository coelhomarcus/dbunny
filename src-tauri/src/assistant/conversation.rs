//! A conversation: the database session it works on, its options and the token that lets
//! its tool calls reach that session and no other.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rand::RngCore;
use serde::Deserialize;

pub const MIN_ROWS: usize = 10;
pub const MAX_ROWS: usize = 1000;

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationOptions {
    /// Every query waits for the person's approval.
    pub ask_before_queries: bool,
    /// The agent sees column names and counts, never values.
    pub schema_only: bool,
    /// Rows a query returns to the agent, from 10 to 1000.
    pub row_limit: usize,
    /// What the person wrote about this connection.
    pub instructions: String,
}

pub struct Conversation {
    pub id: String,
    /// The id of the database session its tools act on.
    pub driver_session: String,
    /// The agent's own id for the conversation, once it has one.
    pub acp_session: Mutex<Option<String>>,
    pub token: String,
    pub options: ConversationOptions,
    /// "Allow reads in this chat" was chosen.
    pub reads_allowed: AtomicBool,
}

#[derive(Default)]
pub struct Conversations {
    items: Mutex<Vec<Arc<Conversation>>>,
}

impl Conversations {
    pub fn create(&self, driver_session: &str, options: ConversationOptions) -> Arc<Conversation> {
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let conversation = Arc::new(Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            driver_session: driver_session.to_string(),
            acp_session: Mutex::new(None),
            token: secret.iter().map(|byte| format!("{byte:02x}")).collect(),
            options: ConversationOptions { row_limit: options.row_limit.clamp(MIN_ROWS, MAX_ROWS), ..options },
            reads_allowed: AtomicBool::new(false),
        });
        self.items.lock().unwrap().push(Arc::clone(&conversation));
        conversation
    }

    /// The conversation a token belongs to, comparing in constant time.
    pub fn by_token(&self, token: &str) -> Option<Arc<Conversation>> {
        let items = self.items.lock().unwrap();
        let mut found = None;
        for conversation in items.iter() {
            if same_secret(&conversation.token, token) {
                found = Some(Arc::clone(conversation));
            }
        }
        found
    }

    pub fn get(&self, id: &str) -> Option<Arc<Conversation>> {
        self.items.lock().unwrap().iter().find(|c| c.id == id).cloned()
    }

    pub fn by_acp_session(&self, acp_session: &str) -> Option<Arc<Conversation>> {
        self.items.lock().unwrap().iter().find(|c| c.acp_session.lock().unwrap().as_deref() == Some(acp_session)).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Conversation>> {
        let mut items = self.items.lock().unwrap();
        let at = items.iter().position(|c| c.id == id)?;
        Some(items.remove(at))
    }

    /// Takes every conversation that works on database session `driver_session`.
    pub fn remove_for_session(&self, driver_session: &str) -> Vec<Arc<Conversation>> {
        let mut items = self.items.lock().unwrap();
        let (taken, kept): (Vec<_>, Vec<_>) = items.drain(..).partition(|c| c.driver_session == driver_session);
        *items = kept;
        taken
    }
}

/// Whether two secrets are equal, taking as long for a near miss as for a far one.
fn same_secret(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |difference, (x, y)| difference | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(rows: usize) -> ConversationOptions {
        ConversationOptions { ask_before_queries: true, schema_only: false, row_limit: rows, instructions: String::new() }
    }

    #[test]
    fn tokens_are_64_hex_chars_and_unique() {
        let all = Conversations::default();
        let (a, b) = (all.create("s1", options(100)), all.create("s1", options(100)));
        for token in [&a.token, &b.token] {
            assert_eq!(token.len(), 64);
            assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_ne!(a.token, b.token);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn by_token_finds_and_wrong_token_does_not() {
        let all = Conversations::default();
        let a = all.create("s1", options(100));
        assert_eq!(all.by_token(&a.token).unwrap().id, a.id);
        assert!(all.by_token(&"0".repeat(64)).is_none());
        assert!(all.by_token("").is_none());
        assert!(all.by_token(&a.token[..63]).is_none());
    }

    #[test]
    fn removed_conversation_token_no_longer_works() {
        let all = Conversations::default();
        let a = all.create("s1", options(100));
        assert!(all.remove(&a.id).is_some());
        assert!(all.by_token(&a.token).is_none() && all.get(&a.id).is_none());
        assert!(all.remove(&a.id).is_none());
    }

    #[test]
    fn remove_for_session_takes_only_that_session() {
        let all = Conversations::default();
        let (a, b, c) = (all.create("s1", options(100)), all.create("s2", options(100)), all.create("s1", options(100)));
        let removed: Vec<String> = all.remove_for_session("s1").into_iter().map(|c| c.id.clone()).collect();
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&a.id) && removed.contains(&c.id));
        assert!(all.get(&b.id).is_some());
    }

    #[test]
    fn row_limit_is_clamped() {
        let all = Conversations::default();
        assert_eq!(all.create("s", options(5)).options.row_limit, MIN_ROWS);
        assert_eq!(all.create("s", options(5000)).options.row_limit, MAX_ROWS);
        assert_eq!(all.create("s", options(250)).options.row_limit, 250);
    }

    #[test]
    fn acp_session_finds_its_conversation() {
        let all = Conversations::default();
        let a = all.create("s1", options(100));
        *a.acp_session.lock().unwrap() = Some("acp-1".into());
        assert_eq!(all.by_acp_session("acp-1").unwrap().id, a.id);
        assert!(all.by_acp_session("acp-2").is_none());
    }
}
