use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::db::postgres::ssh::SshConfig;
use crate::db::Engine;
use super::state::AppState;
use super::storage::{save_connections_to_file, SavedConnection, SavedConnectionInfo, TlsSettings};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionInput {
    pub id: Option<String>,
    pub name: String,
    /// Missing from frontends that only knew PostgreSQL.
    #[serde(default)]
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    #[serde(default)]
    pub tls: Option<TlsSettings>,
    pub ssl: bool,
    #[serde(default)]
    pub read_only: bool,
    pub color: String,
    #[serde(default)]
    pub icon: String,
}

#[tauri::command]
pub async fn get_saved_connections(
    state: State<'_, AppState>,
) -> Result<Vec<SavedConnectionInfo>, String> {
    let connections = state.saved_connections.lock().await;

    Ok(connections
        .iter()
        .filter_map(|c| c.to_info(&state.encryption_key).ok())
        .collect())
}

#[tauri::command]
pub async fn save_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    connection: SavedConnectionInput,
) -> Result<SavedConnectionInfo, String> {
    let mut connections = state.saved_connections.lock().await;
    // Restored if the file can't be written, so memory and disk don't disagree
    let previous = connections.clone();
    let now = chrono::Utc::now().to_rfc3339();

    let saved = if let Some(ref id) = connection.id {
        let pos = connections
            .iter()
            .position(|c| &c.id == id)
            .ok_or_else(|| format!("Connection with id {} not found", id))?;

        let created_at = connections[pos].created_at.clone();

        let (read_only, icon) = (connection.read_only, connection.icon.clone());
        connections[pos] = SavedConnection::new(
            &state.encryption_key,
            connection.engine,
            id.clone(),
            connection.name,
            connection.host,
            connection.port,
            connection.database,
            connection.user,
            connection.password,
            connection.ssh,
            connection.tls,
            connection.ssl,
            connection.color,
            created_at,
            now,
        )?;
        connections[pos].read_only = read_only;
        connections[pos].icon = icon;

        connections[pos].clone()
    } else {
        let (read_only, icon) = (connection.read_only, connection.icon.clone());
        let mut new_connection = SavedConnection::new(
            &state.encryption_key,
            connection.engine,
            Uuid::new_v4().to_string(),
            connection.name,
            connection.host,
            connection.port,
            connection.database,
            connection.user,
            connection.password,
            connection.ssh,
            connection.tls,
            connection.ssl,
            connection.color,
            now.clone(),
            now,
        )?;
        new_connection.read_only = read_only;
        new_connection.icon = icon;

        connections.push(new_connection.clone());
        new_connection
    };

    if let Err(e) = save_connections_to_file(&app_handle, &connections) {
        *connections = previous;
        return Err(format!("The connection wasn't saved: {e}"));
    }

    saved.to_info(&state.encryption_key)
}

#[tauri::command]
pub async fn delete_saved_connection(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let mut connections = state.saved_connections.lock().await;
    let previous = connections.clone();
    connections.retain(|c| c.id != id);

    if connections.len() == previous.len() {
        return Err(format!("Connection with id {} not found", id));
    }

    if let Err(e) = save_connections_to_file(&app_handle, &connections) {
        *connections = previous;
        return Err(format!("The connection wasn't deleted: {e}"));
    }

    Ok(true)
}
