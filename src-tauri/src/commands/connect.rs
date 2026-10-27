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
use crate::db::version::{past_end_of_support, EndOfSupport};
use crate::db::{Capabilities, Driver, Engine};
use crate::error::AppError;

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
    /// The server's version line is past its vendor's end of support (and since when).
    pub end_of_support: Option<EndOfSupport>,
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
            end_of_support: driver.version().and_then(|version| past_end_of_support(driver.engine(), version, chrono::Local::now().date_naive())),
        }
    }
}

/// The URL of a PostgreSQL connection must be there and use a PostgreSQL protocol.
fn check_connection_url(connection_url: &str) -> Result<(), AppError> {
    if connection_url.is_empty() {
        return Err(AppError::coded("connection.urlRequired", "connectionUrl is required"));
    }
    if !connection_url.starts_with("postgres://") && !connection_url.starts_with("postgresql://") {
        return Err(AppError::coded("connection.invalidProtocol", "Invalid protocol. Must be postgres:// or postgresql://"));
    }
    Ok(())
}

fn parse_connection_url(connection_url: &str) -> Result<url::Url, AppError> {
    check_connection_url(connection_url)?;
    url::Url::parse(connection_url).map_err(|e| AppError::coded("connection.invalidUrl", format!("Invalid URL: {e}")).param("detail", e.to_string()))
}

fn check_path(path: &str) -> Result<(), AppError> {
    if path.is_empty() {
        return Err(AppError::coded("connection.pathRequired", "path is required"));
    }
    Ok(())
}

fn check_host(host: &str) -> Result<(), AppError> {
    if host.trim().is_empty() {
        return Err(AppError::coded("connection.hostRequired", "host is required"));
    }
    Ok(())
}

#[tauri::command]
pub async fn connect(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_url: String,
    ssh: Option<SshConfig>,
    trust_host_key: Option<String>,
) -> Result<ConnectResponse, AppError> {
    let url = parse_connection_url(&connection_url)?;

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
) -> Result<ConnectResponse, AppError> {
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
) -> Result<ConnectResponse, AppError> {
    check_path(&path)?;
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
fn server_endpoint(app_handle: &tauri::AppHandle, params: MysqlConnectParams, ssh: Option<SshConfig>) -> Result<MysqlEndpoint, AppError> {
    check_host(&params.host)?;
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
        engine: None,
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
) -> Result<ConnectResponse, AppError> {
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
) -> Result<ConnectResponse, AppError> {
    let endpoint = server_endpoint(&app_handle, params, ssh)?;
    let driver = MariadbDriver::open(endpoint, trust_host_key).await?;
    let session_id = state.register(driver.clone()).await?;
    Ok(ConnectResponse::new(session_id, driver.as_ref()))
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    assistant: State<'_, crate::assistant::Assistant>,
    session_id: String,
) -> Result<bool, String> {
    // Its conversations end first: they act on this session
    assistant.close_for_session(&session_id).await;
    if let Some(driver) = state.remove(&session_id).await {
        driver.close().await;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_version_has_no_end_of_support() {
        // The assistant's fake database tells no version
        let response = ConnectResponse::new("s".into(), crate::assistant::fake::FakeDb::new().as_ref());
        assert_eq!(response.end_of_support, None);
    }

    #[test]
    fn an_empty_url_is_a_coded_error() {
        let error = check_connection_url("").unwrap_err();
        assert_eq!((error.code(), error.message()), (Some("connection.urlRequired"), "connectionUrl is required"));
    }

    #[test]
    fn another_protocol_is_a_coded_error() {
        let error = check_connection_url("mysql://localhost").unwrap_err();
        assert_eq!((error.code(), error.message()), (Some("connection.invalidProtocol"), "Invalid protocol. Must be postgres:// or postgresql://"));
        assert!(check_connection_url("postgres://u@h/db").is_ok() && check_connection_url("postgresql://u@h/db").is_ok());
    }

    #[test]
    fn a_url_that_does_not_parse_says_why() {
        let error = parse_connection_url("postgres://exa mple.com:notaport/db").unwrap_err();
        assert_eq!(error.code(), Some("connection.invalidUrl"));
        assert!(error.message().starts_with("Invalid URL: "));
        assert!(serde_json::to_value(&error).unwrap()["params"]["detail"].is_string());
    }

    #[test]
    fn an_empty_path_is_a_coded_error() {
        let error = check_path("").unwrap_err();
        assert_eq!((error.code(), error.message()), (Some("connection.pathRequired"), "path is required"));
        assert!(check_path("/tmp/a.db").is_ok());
    }

    #[test]
    fn a_blank_host_is_a_coded_error() {
        let error = check_host("   ").unwrap_err();
        assert_eq!((error.code(), error.message()), (Some("connection.hostRequired"), "host is required"));
        assert!(check_host("db.local").is_ok());
    }

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
