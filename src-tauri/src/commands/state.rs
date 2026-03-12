use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::storage::{derive_key, SavedConnection};

pub struct Session {
    pub client: Client,
}

pub struct AppState {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub saved_connections: Arc<Mutex<Vec<SavedConnection>>>,
    pub encryption_key: [u8; 32],
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            saved_connections: Arc::new(Mutex::new(Vec::new())),
            encryption_key: derive_key(),
        }
    }
}
