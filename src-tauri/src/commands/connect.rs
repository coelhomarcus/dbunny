use serde::Serialize;
use tauri::State;

use super::state::AppState;
use crate::db::net::ssh::SshConfig;
use crate::db::postgres::{Endpoint, PostgresDriver};
use crate::db::sqlite::{OpenOptions, SqliteDriver};
use crate::db::{Capabilities, Driver, Engine};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    pub session_id: String,
    pub database: String,
    pub host: String,
    pub port: u16,
    pub server_version: String,
    /// The session can't write: the connection asked for read-only transactions
    /// (`options=-c default_transaction_read_only=on`) or the server is a read replica.
    pub read_only: bool,
    pub engine: Engine,
    /// What the session supports, so the app shows only the screens and actions that work.
    pub capabilities: Capabilities,
}

impl ConnectResponse {
    /// What the app learns about a session it just opened.
    pub(crate) fn new(session_id: String, driver: &dyn Driver) -> Self {
        let info = driver.info();
        ConnectResponse {
            session_id,
            database: info.database,
            host: info.host,
            port: info.port,
            server_version: info.server_version,
            read_only: info.read_only,
            engine: driver.engine(),
            capabilities: driver.capabilities().clone(),
        }
    }
}

#[tauri::command]
pub async fn connect(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_url: String,
    ssh: Option<SshConfig>,
    trust_host_key: Option<String>,
) -> Result<ConnectResponse, String> {
    if connection_url.is_empty() {
        return Err("connectionUrl is required".into());
    }

    if !connection_url.starts_with("postgres://") && !connection_url.starts_with("postgresql://") {
        return Err("Invalid protocol. Must be postgres:// or postgresql://".into());
    }

    let url = url::Url::parse(&connection_url).map_err(|e| format!("Invalid URL: {}", e))?;

    let known_hosts = match ssh {
        Some(_) => {
            use tauri::Manager;
            let dir = app_handle
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to get app data dir: {e}"))?;
            Some(dir.join("ssh_known_hosts.json"))
        }
        None => None,
    };
    let endpoint = Endpoint { url, ssh, known_hosts };
    open_session(&state, endpoint, trust_host_key).await
}

/// Connects to `endpoint` and registers the new session.
pub(crate) async fn open_session(
    state: &AppState,
    endpoint: Endpoint,
    trust_host_key: Option<String>,
) -> Result<ConnectResponse, String> {
    let driver = PostgresDriver::open(endpoint, trust_host_key).await?;
    let session_id = state.register(driver.clone()).await?;
    Ok(ConnectResponse::new(session_id, driver.as_ref()))
}

/// Opens the SQLite file at `path` as a new session. `create` makes the file when it
/// doesn't exist (a new database); otherwise a missing file is an error.
#[tauri::command]
pub async fn connect_sqlite(
    state: State<'_, AppState>,
    path: String,
    read_only: bool,
    create: bool,
) -> Result<ConnectResponse, String> {
    if path.is_empty() {
        return Err("path is required".into());
    }
    let opts = OpenOptions { read_only, create, ..Default::default() };
    let driver = SqliteDriver::open(path.into(), opts).await?;
    let session_id = state.register(driver.clone()).await?;
    Ok(ConnectResponse::new(session_id, driver.as_ref()))
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    if let Some(driver) = state.remove(&session_id).await {
        driver.close().await;
    }
    Ok(true)
}
