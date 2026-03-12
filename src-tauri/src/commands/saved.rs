use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use super::state::AppState;
use super::storage::{
    encrypt_password, save_connections_to_file, SavedConnection, SavedConnectionInfo,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionInput {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssl: bool,
    pub color: String,
}

#[tauri::command]
pub async fn get_saved_connections(
    state: State<'_, AppState>,
) -> Result<Vec<SavedConnectionInfo>, String> {
    let connections = state.saved_connections.lock().await;

    connections
        .iter()
        .map(|c| c.to_info(&state.encryption_key))
        .collect::<Result<Vec<_>, _>>()
}

#[tauri::command]
pub async fn save_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    connection: SavedConnectionInput,
) -> Result<SavedConnectionInfo, String> {
    let mut connections = state.saved_connections.lock().await;
    let now = chrono::Utc::now().to_rfc3339();

    let encrypted_password = if connection.password.is_empty() {
        String::new()
    } else {
        encrypt_password(&state.encryption_key, &connection.password)?
    };

    let saved = if let Some(ref id) = connection.id {
        let pos = connections
            .iter()
            .position(|c| &c.id == id)
            .ok_or_else(|| format!("Connection with id {} not found", id))?;

        connections[pos] = SavedConnection {
            id: id.clone(),
            name: connection.name,
            host: connection.host,
            port: connection.port,
            database: connection.database,
            user: connection.user,
            encrypted_password,
            ssl: connection.ssl,
            color: connection.color,
            created_at: connections[pos].created_at.clone(),
            updated_at: now,
        };

        connections[pos].clone()
    } else {
        let new_connection = SavedConnection {
            id: Uuid::new_v4().to_string(),
            name: connection.name,
            host: connection.host,
            port: connection.port,
            database: connection.database,
            user: connection.user,
            encrypted_password,
            ssl: connection.ssl,
            color: connection.color,
            created_at: now.clone(),
            updated_at: now,
        };

        connections.push(new_connection.clone());
        new_connection
    };

    // Persist to disk (write-through)
    let _ = save_connections_to_file(&app_handle, &connections);

    saved.to_info(&state.encryption_key)
}

#[tauri::command]
pub async fn delete_saved_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let mut connections = state.saved_connections.lock().await;
    let len_before = connections.len();
    connections.retain(|c| c.id != id);

    if connections.len() == len_before {
        return Err(format!("Connection with id {} not found", id));
    }

    // Persist to disk
    let _ = save_connections_to_file(&app_handle, &connections);

    Ok(true)
}
