use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::storage::{get_or_create_key, SavedConnection};

pub struct Session {
    // Shared so commands can use the client without holding the sessions lock
    pub client: Arc<Client>,
}

pub struct AppState {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub saved_connections: Arc<Mutex<Vec<SavedConnection>>>,
    pub encryption_key: [u8; 32],
}

impl AppState {
    pub fn new() -> Self {
        let encryption_key = get_or_create_key()
            .expect("Failed to derive encryption key");

        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            saved_connections: Arc::new(Mutex::new(Vec::new())),
            encryption_key,
        }
    }

    /// Client of a session. The sessions lock is released before returning, so a
    /// slow query on one connection doesn't block commands on the others.
    pub async fn client(&self, session_id: &str) -> Result<Arc<Client>, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| Arc::clone(&session.client))
            .ok_or_else(|| "Session not found".to_string())
    }
}
