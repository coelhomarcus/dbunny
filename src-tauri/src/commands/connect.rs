use postgres_native_tls::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::config::SslMode;
use uuid::Uuid;

use super::state::{AppState, Link, Notice, Notices, Session};

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

/// Builds the driver config and TLS connector from a postgres:// URL.
pub(crate) fn connection_config(url: &url::Url) -> Result<(tokio_postgres::Config, MakeTlsConnector), String> {
    let sslmode = url
        .query_pairs()
        .find(|(key, _)| key == "sslmode")
        .map(|(_, value)| value.into_owned());
    let plan = tls_plan(sslmode.as_deref())?;

    // The driver only understands disable/prefer/require, so sslmode is applied by hand
    let mut stripped = url.clone();
    let others: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "sslmode")
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

    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(!plan.verify_cert)
        .danger_accept_invalid_hostnames(!plan.verify_host)
        .build()
        .map_err(|e| format!("TLS setup failed: {}", e))?;

    Ok((config, MakeTlsConnector::new(connector)))
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

    let (config, tls) = connection_config(&url)?;
    let link = Link { config, tls };
    let notices: Notices = Default::default();
    let client = open_connection(&link, &notices)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;

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
