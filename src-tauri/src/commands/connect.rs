use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::NoTls;
use uuid::Uuid;

use super::state::AppState;
use super::state::Session;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    pub session_id: String,
    pub database: String,
    pub host: String,
    pub port: u16,
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    connection_url: String,
) -> Result<ConnectResponse, String> {
    if connection_url.is_empty() {
        return Err("connectionUrl is required".into());
    }

    if !connection_url.starts_with("postgres://") && !connection_url.starts_with("postgresql://") {
        return Err("Invalid protocol. Must be postgres:// or postgresql://".into());
    }

    // Parse URL for metadata
    let url = url::Url::parse(&connection_url).map_err(|e| format!("Invalid URL: {}", e))?;
    let database = url.path().trim_start_matches('/').to_string();
    let database = if database.is_empty() {
        "postgres".to_string()
    } else {
        database
    };
    let host = url.host_str().unwrap_or("localhost").to_string();
    let port = url.port().unwrap_or(5432);

    let (client, connection) = tokio_postgres::connect(&connection_url, NoTls)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;

    // Spawn the connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    // Test connection + pre-warm catalog cache
    client
        .query(
            "SELECT c.table_name, p.proname \
             FROM information_schema.columns c \
             JOIN pg_proc p ON true \
             WHERE false",
            &[],
        )
        .await
        .map_err(|e| format!("Connection test failed: {}", e))?;

    let session_id = Uuid::new_v4().to_string();

    let session = Session { client };

    let mut sessions = state.sessions.lock().await;
    if sessions.len() >= 50 {
        return Err("Maximum number of concurrent sessions reached".into());
    }
    sessions.insert(session_id.clone(), session);

    Ok(ConnectResponse {
        session_id,
        database,
        host,
        port,
    })
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let mut sessions = state.sessions.lock().await;
    sessions.remove(&session_id);
    Ok(true)
}
