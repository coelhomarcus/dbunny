use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::storage::SavedConnection;
use crate::db::Driver;
use crate::error::AppError;

/// Most sessions open at once.
const MAX_SESSIONS: usize = 50;

/// Whether one more session may open when `open` are already there.
fn check_room(open: usize) -> Result<(), AppError> {
    if open >= MAX_SESSIONS {
        return Err(AppError::coded("connection.limitReached", "Maximum number of concurrent sessions reached"));
    }
    Ok(())
}

pub struct AppState {
    /// Open sessions by id. A command takes its driver out and lets go of the map, so a
    /// slow query on one session doesn't block the others.
    pub sessions: Arc<Mutex<HashMap<String, Arc<dyn Driver>>>>,
    pub saved_connections: Arc<Mutex<Vec<SavedConnection>>>,
    pub encryption_key: [u8; 32],
}

impl AppState {
    pub fn new(encryption_key: [u8; 32]) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            saved_connections: Arc::new(Mutex::new(Vec::new())),
            encryption_key,
        }
    }

    pub async fn driver(&self, session_id: &str) -> Result<Arc<dyn Driver>, String> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| "Session not found".to_string())
    }

    /// Keeps an opened driver as a new session and returns its id.
    pub async fn register(&self, driver: Arc<dyn Driver>) -> Result<String, AppError> {
        let mut sessions = self.sessions.lock().await;
        check_room(sessions.len())?;
        let session_id = Uuid::new_v4().to_string();
        sessions.insert(session_id.clone(), driver);
        Ok(session_id)
    }

    /// Forgets a session; the caller closes it.
    pub async fn remove(&self, session_id: &str) -> Option<Arc<dyn Driver>> {
        self.sessions.lock().await.remove(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_limit_is_a_coded_error() {
        assert!(check_room(MAX_SESSIONS - 1).is_ok());
        let error = check_room(MAX_SESSIONS).unwrap_err();
        assert_eq!((error.code(), error.message()), (Some("connection.limitReached"), "Maximum number of concurrent sessions reached"));
    }
}
