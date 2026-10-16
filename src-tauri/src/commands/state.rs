use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::storage::{get_or_create_key, SavedConnection};

pub struct Session {
    // Shared so commands can use the client without holding the sessions lock
    pub client: Arc<Client>,
    /// How to reach the server again: opening the transaction connection, cancelling queries.
    pub link: Link,
    /// Messages like `RAISE NOTICE`, from every connection of the session.
    pub notices: Notices,
    /// Separate connection for a transaction opened from the query editor, so table views
    /// and the explorer keep working outside it. None when no transaction is open.
    pub transaction: Option<Arc<Client>>,
}

#[derive(Clone)]
pub struct Link {
    pub config: tokio_postgres::Config,
    pub tls: postgres_native_tls::MakeTlsConnector,
}

/// A notice, warning or other non-error message sent by the server.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

pub type Notices = Arc<std::sync::Mutex<Vec<Notice>>>;

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

    /// Client for the query editor: the transaction connection while one is open.
    pub async fn editor_client(&self, session_id: &str) -> Result<(Arc<Client>, bool), String> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id).ok_or_else(|| "Session not found".to_string())?;
        Ok(match &session.transaction {
            Some(tx) => (Arc::clone(tx), true),
            None => (Arc::clone(&session.client), false),
        })
    }

    pub async fn notices(&self, session_id: &str) -> Result<Notices, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| Arc::clone(&session.notices))
            .ok_or_else(|| "Session not found".to_string())
    }

    pub async fn link(&self, session_id: &str) -> Result<Link, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.link.clone())
            .ok_or_else(|| "Session not found".to_string())
    }
}
