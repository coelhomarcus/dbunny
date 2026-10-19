use serde::{Deserialize, Serialize};
use tauri::State;

use super::state::AppState;
use crate::db::mariadb::MariadbDriver;
use crate::db::mysql::MysqlDriver;
use crate::db::mysql_common::connect::MysqlEndpoint;
use crate::db::net::ssh::SshConfig;
use crate::db::net::tls::TlsSettings;
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

    let known_hosts = known_hosts_for(&app_handle, &ssh)?;
    let endpoint = Endpoint { url, ssh, known_hosts };
    open_session(&state, endpoint, trust_host_key).await
}

/// The app's SSH known hosts file, when the connection goes through SSH.
fn known_hosts_for(app_handle: &tauri::AppHandle, ssh: &Option<SshConfig>) -> Result<Option<std::path::PathBuf>, String> {
    match ssh {
        Some(_) => {
            use tauri::Manager;
            let dir = app_handle
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to get app data dir: {e}"))?;
            Ok(Some(dir.join("ssh_known_hosts.json")))
        }
        None => Ok(None),
    }
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

/// The connection form of a MySQL server. An empty database opens the first one the user can see.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MysqlConnectParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub read_only: bool,
    pub tls: Option<TlsSettings>,
}

/// The endpoint a server connection form describes.
fn server_endpoint(app_handle: &tauri::AppHandle, params: MysqlConnectParams, ssh: Option<SshConfig>) -> Result<MysqlEndpoint, String> {
    if params.host.trim().is_empty() {
        return Err("host is required".into());
    }
    let known_hosts = known_hosts_for(app_handle, &ssh)?;
    let database = params.database.trim();
    Ok(MysqlEndpoint {
        host: params.host.trim().to_string(),
        port: params.port,
        database: (!database.is_empty()).then(|| database.to_string()),
        user: params.user,
        password: params.password,
        tls: params.tls,
        ssh,
        known_hosts,
        read_only: params.read_only,
    })
}

/// Connects to a MySQL server, optionally through SSH. `trust_host_key` is an SSH host
/// key fingerprint the user confirmed.
#[tauri::command]
pub async fn connect_mysql(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    params: MysqlConnectParams,
    ssh: Option<SshConfig>,
    trust_host_key: Option<String>,
) -> Result<ConnectResponse, String> {
    let endpoint = server_endpoint(&app_handle, params, ssh)?;
    let driver = MysqlDriver::open(endpoint, trust_host_key).await?;
    let session_id = state.register(driver.clone()).await?;
    Ok(ConnectResponse::new(session_id, driver.as_ref()))
}

/// Connects to a MariaDB server, optionally through SSH; the form is the MySQL one.
#[tauri::command]
pub async fn connect_mariadb(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    params: MysqlConnectParams,
    ssh: Option<SshConfig>,
    trust_host_key: Option<String>,
) -> Result<ConnectResponse, String> {
    let endpoint = server_endpoint(&app_handle, params, ssh)?;
    let driver = MariadbDriver::open(endpoint, trust_host_key).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What the connection modal sends for `connect_mysql` (see `mysqlParamsOf` in the frontend).
    #[test]
    fn mysql_params_read_what_the_frontend_sends() {
        let sent = serde_json::json!({
            "host": "db.local", "port": 3307, "database": "", "user": "ana", "password": "pw", "readOnly": true,
            "tls": { "mode": "verify-ca", "rootCert": "/ca.pem", "cert": "", "key": "" }
        });
        let params: MysqlConnectParams = serde_json::from_value(sent).unwrap();
        assert_eq!((params.host.as_str(), params.port, params.database.as_str(), params.read_only), ("db.local", 3307, "", true));
        let tls = params.tls.unwrap();
        assert_eq!((tls.mode.as_str(), tls.root_cert.as_str()), ("verify-ca", "/ca.pem"));

        // Without SSL the frontend sends null
        let plain: MysqlConnectParams =
            serde_json::from_value(serde_json::json!({ "host": "h", "port": 3306, "database": "d", "user": "u", "password": "", "readOnly": false, "tls": null })).unwrap();
        assert!(plain.tls.is_none() && !plain.read_only);
    }
}
