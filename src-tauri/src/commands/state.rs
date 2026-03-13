use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use super::storage::{get_or_create_key, SavedConnection};

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
        let encryption_key = get_or_create_key()
            .expect("Failed to access system keychain for encryption key");

        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            saved_connections: Arc::new(Mutex::new(Vec::new())),
            encryption_key,
        }
    }
}
