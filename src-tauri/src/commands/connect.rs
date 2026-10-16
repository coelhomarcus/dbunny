use postgres_native_tls::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::config::SslMode;
use uuid::Uuid;

use super::ssh::{open_tunnel, KnownHosts, SshConfig, Tunnel};
use super::state::{AppState, Endpoint, Link, Notice, Notices, Session};

#[derive(Serialize, Deserialize)]
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
}

/// How the TLS handshake should behave, derived from the URL's `sslmode`.
#[derive(Debug, PartialEq)]
struct TlsPlan {
    mode: SslMode,
    verify_cert: bool,
    verify_host: bool,
}

/// Maps libpq's `sslmode` to a TLS plan. Like libpq, `prefer` and `require` encrypt
/// without validating the certificate (managed databases often use their own CA);
/// `verify-ca` and `verify-full` do validate it.
fn tls_plan(sslmode: Option<&str>) -> Result<TlsPlan, String> {
    let plan = |mode, verify_cert, verify_host| TlsPlan { mode, verify_cert, verify_host };
    match sslmode.unwrap_or("prefer") {
        "disable" => Ok(plan(SslMode::Disable, false, false)),
        "prefer" => Ok(plan(SslMode::Prefer, false, false)),
        "require" => Ok(plan(SslMode::Require, false, false)),
        "verify-ca" => Ok(plan(SslMode::Require, true, false)),
        "verify-full" => Ok(plan(SslMode::Require, true, true)),
        other => Err(format!(
            "Invalid sslmode \"{}\". Use disable, prefer, require, verify-ca or verify-full",
            other
        )),
    }
}

/// URL parameters DBunny applies itself instead of passing them to the driver
/// (which only knows a few sslmodes and no certificate files).
const TLS_PARAMS: [&str; 4] = ["sslmode", "sslrootcert", "sslcert", "sslkey"];

fn read_file(label: &str, path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("Couldn't read the {label} {path}: {e}"))
}

/// Builds the driver config and TLS connector from a postgres:// URL.
pub(crate) fn connection_config(url: &url::Url) -> Result<(tokio_postgres::Config, MakeTlsConnector), String> {
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty())
    };
    let mut plan = tls_plan(param("sslmode").as_deref())?;
    let root_cert = param("sslrootcert");
    // Like libpq: with a CA file, "require" checks the certificate against it
    if root_cert.is_some() && plan.mode == SslMode::Require {
        plan.verify_cert = true;
    }

    let mut stripped = url.clone();
    let others: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !TLS_PARAMS.contains(&key.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if others.is_empty() {
        stripped.set_query(None);
    } else {
        stripped.query_pairs_mut().clear().extend_pairs(others);
        // That writes spaces as "+" (form encoding), which the driver reads literally;
        // a real "+" is already "%2B", so every "+" left is a space
        let query = stripped.query().unwrap_or("").replace('+', "%20");
        stripped.set_query(Some(&query));
    }

    let mut config: tokio_postgres::Config = stripped
        .as_str()
        .parse()
        .map_err(|e| format!("Invalid connection URL: {}", e))?;
    config.ssl_mode(plan.mode);

    let mut builder = native_tls::TlsConnector::builder();
    builder
        .danger_accept_invalid_certs(!plan.verify_cert)
        .danger_accept_invalid_hostnames(!plan.verify_host);
    if let Some(path) = &root_cert {
        let ca = native_tls::Certificate::from_pem(&read_file("CA certificate", path)?)
            .map_err(|e| format!("The CA certificate {path} isn't a PEM certificate: {e}"))?;
        builder.add_root_certificate(ca);
    }
    match (param("sslcert"), param("sslkey")) {
        (Some(cert), Some(key)) => {
            let identity = native_tls::Identity::from_pkcs8(&read_file("client certificate", &cert)?, &read_file("client key", &key)?)
                .map_err(|e| {
                    format!(
                        "Couldn't use the client certificate and key: {e}. The key must be an unencrypted PKCS#8 PEM \
                         (convert it with: openssl pkcs8 -topk8 -nocrypt -in key.pem -out key.pk8.pem)"
                    )
                })?;
            builder.identity(identity);
        }
        (None, None) => {}
        _ => return Err("A client certificate needs both the certificate and its key".into()),
    }
    let connector = builder.build().map_err(|e| format!("TLS setup failed: {}", e))?;

    Ok((config, MakeTlsConnector::new(connector)))
}

/// Opens the session's connection: the SSH tunnel first when there is one, then the
/// database. Used on connect and again to reconnect after the connection was lost.
pub(crate) async fn establish(
    endpoint: &Endpoint,
    notices: &Notices,
    trust_host_key: Option<String>,
) -> Result<(tokio_postgres::Client, Link, Option<std::sync::Arc<Tunnel>>), String> {
    let host = endpoint.url.host_str().unwrap_or("localhost").to_string();
    let port = endpoint.url.port().unwrap_or(5432);

    // Through SSH, the driver connects to a local port forwarded to the database host
    let (url, tunnel) = match (&endpoint.ssh, &endpoint.known_hosts) {
        (Some(ssh), Some(known_hosts)) => {
            let known_hosts = KnownHosts::new(known_hosts.clone());
            let tunnel = open_tunnel(ssh, (host, port), known_hosts, trust_host_key).await?;
            let url = through_tunnel(&endpoint.url, tunnel.local_port)?;
            (url, Some(std::sync::Arc::new(tunnel)))
        }
        _ => (endpoint.url.clone(), None),
    };

    let (mut config, tls) = connection_config(&url)?;
    // Notice a dead connection (sleep, network change) within minutes instead of hours
    config.keepalives_idle(std::time::Duration::from_secs(60));
    let link = Link { config, tls };
    let client = open_connection(&link, notices)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    Ok((client, link, tunnel))
}

/// The URL rewritten to reach the database through a tunnel's local port. The host
/// stays as it was so TLS still checks the database's real name; `hostaddr` is where
/// the driver actually connects.
pub(crate) fn through_tunnel(url: &url::Url, local_port: u16) -> Result<url::Url, String> {
    let mut url = url.clone();
    url.set_port(Some(local_port)).map_err(|_| "Can't tunnel this connection URL".to_string())?;
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "hostaddr")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.push(("hostaddr".into(), "127.0.0.1".into()));
    url.query_pairs_mut().clear().extend_pairs(pairs);
    Ok(url)
}

/// Notices kept per session; the oldest are dropped past this many.
const MAX_NOTICES: usize = 500;

/// Opens a connection and drives it in the background, keeping the server's notices
/// (`RAISE NOTICE`, warnings) instead of dropping them.
pub(crate) async fn open_connection(link: &Link, notices: &Notices) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
    use futures_util::StreamExt;
    use tokio_postgres::AsyncMessage;

    let (client, mut connection) = link.config.connect(link.tls.clone()).await?;
    let notices = std::sync::Arc::clone(notices);
    tokio::spawn(async move {
        let mut messages = futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
        while let Some(message) = messages.next().await {
            match message {
                Ok(AsyncMessage::Notice(notice)) => {
                    let mut list = notices.lock().unwrap();
                    if list.len() >= MAX_NOTICES {
                        list.remove(0);
                    }
                    list.push(Notice {
                        severity: notice.severity().to_string(),
                        message: notice.message().to_string(),
                        detail: notice.detail().map(str::to_string),
                        hint: notice.hint().map(str::to_string),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                    break;
                }
            }
        }
    });
    Ok(client)
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
async fn open_session(state: &AppState, endpoint: Endpoint, trust_host_key: Option<String>) -> Result<ConnectResponse, String> {
    let database = match endpoint.url.path().trim_start_matches('/') {
        "" => "postgres".to_string(),
        name => percent_decode(name),
    };
    let host = endpoint.url.host_str().unwrap_or("localhost").to_string();
    let port = endpoint.url.port().unwrap_or(5432);
    let notices: Notices = Default::default();
    let (client, link, tunnel) = establish(&endpoint, &notices, trust_host_key).await?;

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

    // Shown in the status bar; not worth failing the connection over
    let server_version: String = client
        .query_one("SHOW server_version", &[])
        .await
        .map(|row| row.get(0))
        .unwrap_or_default();

    let read_only = client
        .query_one("SHOW transaction_read_only", &[])
        .await
        .map(|row| row.get::<_, String>(0) == "on")
        .unwrap_or(false);

    let session_id = Uuid::new_v4().to_string();

    let session = Session {
        client: std::sync::Arc::new(client),
        link,
        notices,
        transaction: None,
        tunnel,
        endpoint,
        reconnect_lock: Default::default(),
    };

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
        server_version,
        read_only,
    })
}

fn percent_decode(text: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", text.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| text.to_string())
}

/// The endpoint of an open session, pointed at another database of the same server.
pub(crate) fn endpoint_for_database(endpoint: &Endpoint, database: &str) -> Result<Endpoint, String> {
    if database.trim().is_empty() {
        return Err("Choose a database".into());
    }
    let mut url = endpoint.url.clone();
    url.path_segments_mut()
        .map_err(|_| "Can't change the database of this connection".to_string())?
        .clear()
        .push(database);
    Ok(Endpoint { url, ..endpoint.clone() })
}

/// Opens a new session on another database of the same server, with the same login,
/// SSL, read-only and SSH settings as `session_id`.
#[tauri::command]
pub async fn connect_database(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> Result<ConnectResponse, String> {
    let endpoint = {
        let sessions = state.sessions.lock().await;
        let session = sessions.get(&session_id).ok_or_else(|| "Session not found".to_string())?;
        endpoint_for_database(&session.endpoint, &database)?
    };
    open_session(&state, endpoint, None).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub size_bytes: Option<i64>,
    pub is_current: bool,
    /// The user may connect to it.
    pub can_connect: bool,
}

#[tauri::command]
pub async fn get_databases(state: State<'_, AppState>, session_id: String) -> Result<Vec<DatabaseInfo>, String> {
    let client = state.client(&session_id).await?;
    list_databases(&client).await
}

pub(crate) async fn list_databases(client: &tokio_postgres::Client) -> Result<Vec<DatabaseInfo>, String> {
    let rows = client
        .query(
            "SELECT datname::text AS name, \
             CASE WHEN has_database_privilege(datname, 'CONNECT') THEN pg_database_size(datname) END AS size_bytes, \
             datname = current_database() AS is_current, has_database_privilege(datname, 'CONNECT') AS can_connect \
             FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY datname",
            &[],
        )
        .await
        .map_err(|e| super::query::db_message(&e))?;
    Ok(rows
        .iter()
        .map(|r| DatabaseInfo {
            name: r.get("name"),
            size_bytes: r.get("size_bytes"),
            is_current: r.get("is_current"),
            can_connect: r.get("can_connect"),
        })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(mode: Option<&str>) -> TlsPlan {
        tls_plan(mode).unwrap()
    }

    #[test]
    fn sslmode_defaults_to_prefer_without_validation() {
        assert_eq!(plan(None), TlsPlan { mode: SslMode::Prefer, verify_cert: false, verify_host: false });
    }

    #[test]
    fn require_encrypts_without_validating_the_certificate() {
        assert_eq!(plan(Some("require")), TlsPlan { mode: SslMode::Require, verify_cert: false, verify_host: false });
    }

    #[test]
    fn verify_modes_validate_the_certificate() {
        assert_eq!(plan(Some("verify-ca")), TlsPlan { mode: SslMode::Require, verify_cert: true, verify_host: false });
        assert_eq!(plan(Some("verify-full")), TlsPlan { mode: SslMode::Require, verify_cert: true, verify_host: true });
    }

    #[test]
    fn disable_turns_tls_off() {
        assert_eq!(plan(Some("disable")).mode, SslMode::Disable);
    }

    #[test]
    fn unknown_sslmode_is_rejected_with_a_helpful_message() {
        let err = tls_plan(Some("sometimes")).unwrap_err();
        assert!(err.contains("sometimes") && err.contains("verify-full"));
    }

    #[test]
    fn config_applies_sslmode_and_keeps_other_params() {
        let url = url::Url::parse("postgresql://u:p%40ss@db.example.com:6543/shop?sslmode=require&connect_timeout=5").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
        assert_eq!(config.get_hosts().len(), 1);
        assert_eq!(config.get_ports(), &[6543]);
        assert_eq!(config.get_dbname(), Some("shop"));
        assert_eq!(config.get_user(), Some("u"));
        assert_eq!(config.get_password(), Some(&b"p@ss"[..]));
        assert_eq!(config.get_connect_timeout().map(|d| d.as_secs()), Some(5));
    }

    #[test]
    fn read_only_option_reaches_the_server_config() {
        let url = url::Url::parse("postgresql://u@localhost/db?sslmode=require&options=-c%20default_transaction_read_only%3Don").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_options(), Some("-c default_transaction_read_only=on"));
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
    }

    #[test]
    fn tunnelled_urls_keep_the_host_for_tls_and_connect_locally() {
        let url = url::Url::parse("postgresql://u:p@db.internal:5432/shop?sslmode=verify-full&options=-c%20default_transaction_read_only%3Don").unwrap();
        let tunnelled = through_tunnel(&url, 51234).unwrap();
        let (config, _) = connection_config(&tunnelled).unwrap();
        assert_eq!(config.get_ports(), &[51234]);
        assert_eq!(config.get_hostaddrs(), &["127.0.0.1".parse::<std::net::IpAddr>().unwrap()]);
        assert!(matches!(&config.get_hosts()[0], tokio_postgres::config::Host::Tcp(h) if h == "db.internal"));
        assert_eq!(config.get_options(), Some("-c default_transaction_read_only=on"));
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
    }

    #[test]
    fn another_database_keeps_everything_else() {
        let endpoint = Endpoint {
            url: url::Url::parse("postgresql://u:p%40ss@db.example.com:6543/shop?sslmode=require").unwrap(),
            ssh: None,
            known_hosts: None,
        };
        let other = endpoint_for_database(&endpoint, "my db/2").unwrap();
        assert_eq!(other.url.as_str(), "postgresql://u:p%40ss@db.example.com:6543/my%20db%2F2?sslmode=require");
        let (config, _) = connection_config(&other.url).unwrap();
        assert_eq!(config.get_dbname(), Some("my db/2"));
        assert_eq!(percent_decode("my%20db%2F2"), "my db/2");
        assert!(endpoint_for_database(&endpoint, " ").is_err());
    }

    #[test]
    fn certificate_params_are_kept_from_the_driver_and_checked() {
        let url = url::Url::parse("postgresql://u@db.example.com/db?sslmode=require&sslrootcert=/nope/ca.pem&application_name=x").unwrap();
        let err = connection_config(&url).err().unwrap();
        assert!(err.contains("CA certificate /nope/ca.pem"), "{err}");

        let url = url::Url::parse("postgresql://u@db.example.com/db?sslmode=require&sslcert=/a.pem").unwrap();
        assert!(connection_config(&url).err().unwrap().contains("both the certificate and its key"));

        // Empty values are ignored, the rest still reaches the driver
        let url = url::Url::parse("postgresql://u@db.example.com/db?sslmode=require&sslrootcert=&application_name=dbunny").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_application_name(), Some("dbunny"));
    }

    #[test]
    fn config_without_query_still_parses() {
        let url = url::Url::parse("postgresql://u@localhost/db").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_ssl_mode(), SslMode::Prefer);
    }

    #[test]
    fn verify_full_url_is_accepted_even_though_the_driver_does_not_know_it() {
        let url = url::Url::parse("postgresql://u@db.example.com/db?sslmode=verify-full").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
    }
}

/// Tests that need a real server with SSL enabled (a self-signed certificate is fine). Run with:
///   DBUNNY_TEST_DATABASE_URL=postgresql://user:pass@localhost:5432/db cargo test --lib -- --ignored
#[cfg(test)]
mod server_tests {
    use super::*;

    fn base_url() -> String {
        std::env::var("DBUNNY_TEST_DATABASE_URL").expect("set DBUNNY_TEST_DATABASE_URL to run the server tests")
    }

    /// Connects with the given sslmode and reports whether the session is encrypted.
    async fn connect_with(sslmode: &str) -> Result<bool, String> {
        let separator = if base_url().contains('?') { '&' } else { '?' };
        let url = url::Url::parse(&format!("{}{}sslmode={}", base_url(), separator, sslmode)).unwrap();
        let (config, tls) = connection_config(&url)?;
        let (client, connection) = config.connect(tls).await.map_err(|e| e.to_string())?;
        tokio::spawn(connection);
        let row = client
            .query_one("SELECT COALESCE((SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()), false)", &[])
            .await
            .map_err(|e| e.to_string())?;
        Ok(row.get(0))
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL (server with ssl on)"]
    async fn require_and_prefer_encrypt_even_with_a_self_signed_certificate() {
        assert!(connect_with("require").await.unwrap());
        assert!(connect_with("prefer").await.unwrap());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL (server with ssl on)"]
    async fn disable_does_not_use_tls() {
        assert!(!connect_with("disable").await.unwrap());
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL (server with ssl on)"]
    async fn verify_modes_refuse_a_certificate_nobody_trusts() {
        assert!(connect_with("verify-ca").await.is_err());
        assert!(connect_with("verify-full").await.is_err());
    }
}

/// Switching databases against a real server, see `query::server_tests`.
#[cfg(test)]
mod database_server_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL and a second database named dbunny_second"]
    async fn another_database_opens_as_its_own_session() {
        let url = url::Url::parse(&std::env::var("DBUNNY_TEST_DATABASE_URL").unwrap()).unwrap();
        let state = AppState {
            sessions: Default::default(),
            saved_connections: Default::default(),
            encryption_key: [0; 32],
        };
        let first = open_session(&state, Endpoint { url, ssh: None, known_hosts: None }, None).await.unwrap();

        let client = state.client(&first.session_id).await.unwrap();
        let databases = list_databases(&client).await.unwrap();
        let names: Vec<_> = databases.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dbunny_second"), "{names:?}");
        assert!(!names.contains(&"template1"));
        assert!(databases.iter().find(|d| d.is_current).unwrap().size_bytes.unwrap() > 0);

        let endpoint = endpoint_for_database(&state.sessions.lock().await[&first.session_id].endpoint, "dbunny_second").unwrap();
        let second = open_session(&state, endpoint, None).await.unwrap();
        assert_eq!(second.database, "dbunny_second");
        assert_ne!(second.session_id, first.session_id);
        let current: String = state.client(&second.session_id).await.unwrap().query_one("SELECT current_database()::text", &[]).await.unwrap().get(0);
        assert_eq!(current, "dbunny_second");
        // The first session is untouched
        let current: String = client.query_one("SELECT current_database()::text", &[]).await.unwrap().get(0);
        assert_eq!(current, first.database);
    }
}

/// Certificates against a real server with SSL and certificate login. Run with:
///   DBUNNY_TEST_SSL_DIR=<dir with ca.pem, other-ca.pem, client.pem, client.pk8.pem, client.key>
///   DBUNNY_TEST_SSL_PORT=<port of a server using a certificate for "localhost" signed by ca.pem,
///   with a "demo"/"demo" password login and a "certuser" that logs in by certificate>
///   cargo test --lib certificate_server_tests -- --ignored
#[cfg(test)]
mod certificate_server_tests {
    use super::*;

    fn dir() -> String {
        std::env::var("DBUNNY_TEST_SSL_DIR").expect("set DBUNNY_TEST_SSL_DIR")
    }

    async fn try_connect(url: &str) -> Result<String, String> {
        let url = url::Url::parse(url).unwrap();
        let (config, tls) = connection_config(&url)?;
        let (client, connection) = config.connect(tls).await.map_err(|e| e.to_string())?;
        tokio::spawn(connection);
        let row = client
            .query_one("SELECT current_user::text || ' ' || ssl::text FROM pg_stat_ssl WHERE pid = pg_backend_pid()", &[])
            .await
            .map_err(|e| e.to_string())?;
        Ok(row.get(0))
    }

    fn url(user: &str, host: &str, query: &str) -> String {
        let port = std::env::var("DBUNNY_TEST_SSL_PORT").expect("set DBUNNY_TEST_SSL_PORT");
        let auth = if user == "demo" { "demo:demo".to_string() } else { user.to_string() };
        format!("postgresql://{auth}@{host}:{port}/demo?{query}")
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSL_DIR and DBUNNY_TEST_SSL_PORT"]
    async fn the_server_certificate_is_checked_against_the_given_ca() {
        let d = dir();
        // The right CA: verify-ca and verify-full (the certificate is for localhost) both pass
        assert_eq!(try_connect(&url("demo", "localhost", &format!("sslmode=verify-ca&sslrootcert={d}/ca.pem"))).await.unwrap(), "demo true");
        assert_eq!(try_connect(&url("demo", "localhost", &format!("sslmode=verify-full&sslrootcert={d}/ca.pem"))).await.unwrap(), "demo true");
        // require + CA behaves like verify-ca, as in libpq
        assert!(try_connect(&url("demo", "localhost", &format!("sslmode=require&sslrootcert={d}/other-ca.pem"))).await.is_err());
        // Another CA is refused
        assert!(try_connect(&url("demo", "localhost", &format!("sslmode=verify-ca&sslrootcert={d}/other-ca.pem"))).await.is_err());
        // verify-full accepts the other name the certificate lists (its IP address)
        assert!(try_connect(&url("demo", "127.0.0.1", &format!("sslmode=verify-full&sslrootcert={d}/ca.pem"))).await.is_ok());
        // Plain require without a CA still encrypts without checking
        assert_eq!(try_connect(&url("demo", "localhost", "sslmode=require")).await.unwrap(), "demo true");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSL_DIR and DBUNNY_TEST_SSL_PORT"]
    async fn client_certificates_log_in() {
        let d = dir();
        let with_cert = format!("sslmode=verify-ca&sslrootcert={d}/ca.pem&sslcert={d}/client.pem&sslkey={d}/client.pk8.pem");
        assert_eq!(try_connect(&url("certuser", "localhost", &with_cert)).await.unwrap(), "certuser true");
        // Without the certificate the server refuses the login
        assert!(try_connect(&url("certuser", "localhost", &format!("sslmode=verify-ca&sslrootcert={d}/ca.pem"))).await.is_err());
        // A PKCS#1 key explains how to convert it
        let pkcs1 = format!("sslmode=require&sslcert={d}/client.pem&sslkey={d}/client.key");
        match try_connect(&url("certuser", "localhost", &pkcs1)).await {
            Ok(_) => {} // some platforms read PKCS#1 too
            Err(e) => assert!(e.contains("PKCS#8") || e.contains("pkcs8"), "{e}"),
        }
    }
}
