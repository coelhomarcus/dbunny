//! Opening a PostgreSQL session: TLS settings from the URL, the SSH tunnel and the
//! connections themselves.

use postgres_native_tls::MakeTlsConnector;
use std::sync::Arc;
use tokio_postgres::config::SslMode;

use crate::db::version::{postgres_is_stock, ServerVersion};
use super::session::{Endpoint, Link, Notices, PgState};
use super::PostgresDriver;
use crate::db::model::Notice;
use crate::db::net::ssh::{open_tunnel, KnownHosts, Tunnel};
use crate::db::net::tls::read_file;
use crate::db::{Capabilities, ConnectionInfo};

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

/// The socket folder when the URL's host is one (`postgresql://user@%2Ftmp/db`, as libpq
/// writes it), None for a host name.
pub(crate) fn socket_dir(url: &url::Url) -> Option<String> {
    let host = percent_decode(url.host_str()?);
    host.starts_with('/').then_some(host)
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
    if socket_dir(&endpoint.url).is_some() {
        if cfg!(windows) {
            return Err("Unix sockets aren't available on Windows: use the host name and port".into());
        }
        if endpoint.ssh.is_some() {
            return Err("A socket is a local file, so it can't be reached through an SSH tunnel".into());
        }
    }

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

impl PostgresDriver {
    /// Connects to `endpoint`: the tunnel and the main connection, a first query that
    /// pre-warms the catalog cache, then what the status bar shows.
    pub async fn open(endpoint: Endpoint, trust_host_key: Option<String>) -> Result<Arc<PostgresDriver>, String> {
        let database = match endpoint.url.path().trim_start_matches('/') {
            "" => "postgres".to_string(),
            name => percent_decode(name),
        };
        // A socket shows as its folder
        let host = socket_dir(&endpoint.url).unwrap_or_else(|| endpoint.url.host_str().unwrap_or("localhost").to_string());
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

        // The version capabilities and the end of support follow; a server that doesn't tell it
        // (unlikely) opens without them rather than not at all
        // Compatible servers (YugabyteDB, Greenplum, CockroachDB, Redshift) report a version they
        // emulate: for them it stays unknown, so no floor and no end of support
        let version_text: String = client.query_one("SELECT version()", &[]).await.map(|row| row.get(0)).unwrap_or_default();
        let version = client
            .query_one("SHOW server_version_num", &[])
            .await
            .ok()
            .and_then(|row| row.get::<_, String>(0).parse::<i32>().ok())
            .map(ServerVersion::from_postgres_num)
            .filter(|_| postgres_is_stock(&server_version, &version_text));
        if let Some(version) = version {
            check_postgres_version(version, server_version.split_whitespace().next().unwrap_or(&server_version))?;
        }

        let read_only = client
            .query_one("SHOW transaction_read_only", &[])
            .await
            .map(|row| row.get::<_, String>(0) == "on")
            .unwrap_or(false);

        let session = PgState {
            client: Arc::new(client),
            link,
            notices,
            transaction: None,
            editor: None,
            reader: None,
            tunnel,
        };

        Ok(Arc::new(PostgresDriver {
            state: tokio::sync::Mutex::new(Some(session)),
            endpoint,
            reconnect_lock: Default::default(),
            reading: Default::default(),
            info: ConnectionInfo { database, host, port, server_version, read_only },
            version,
            // Unknown: what every supported version has
            capabilities: Capabilities::postgres(version.unwrap_or(ServerVersion { major: 13, minor: 0, patch: 0 })),
        }))
    }
}

/// Refuses PostgreSQL older than 13, the oldest the test matrix covers. `shown` is the version as
/// the server writes it.
pub(crate) fn check_postgres_version(version: ServerVersion, shown: &str) -> Result<(), String> {
    if version.major < 13 {
        return Err(format!("PostgreSQL {shown} isn't supported; DBunny needs PostgreSQL 13 or newer."));
    }
    Ok(())
}

fn percent_decode(text: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", text.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::admin::endpoint_for_database;
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
    fn a_socket_folder_as_host_connects_through_the_socket() {
        let url = url::Url::parse("postgresql://me:secret@%2Fvar%2Frun%2Fpostgresql:5433/shop").unwrap();
        assert_eq!(socket_dir(&url).as_deref(), Some("/var/run/postgresql"));
        let (config, _) = connection_config(&url).unwrap();
        #[cfg(unix)]
        assert!(matches!(&config.get_hosts()[0], tokio_postgres::config::Host::Unix(p) if p == std::path::Path::new("/var/run/postgresql")));
        assert_eq!(config.get_ports(), &[5433]);
        assert_eq!(config.get_user(), Some("me"));
        assert_eq!(config.get_dbname(), Some("shop"));
        assert_eq!(socket_dir(&url::Url::parse("postgresql://u@db.example.com/x").unwrap()), None);
    }

    #[test]
    fn encoded_names_reach_the_driver_decoded() {
        // As the form writes them: "#", "?" and "@" in names are percent-encoded
        let url = url::Url::parse("postgresql://admin%40corp:p%23ss@db.example.com:5432/app%232").unwrap();
        let (config, _) = connection_config(&url).unwrap();
        assert_eq!(config.get_user(), Some("admin@corp"));
        assert_eq!(config.get_password(), Some(&b"p#ss"[..]));
        assert_eq!(config.get_dbname(), Some("app#2"));
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

/// Opening sessions against a real server, see `console::server_tests`.
#[cfg(test)]
mod database_server_tests {
    use crate::db::net::ssh::SshConfig;
    use super::*;
    use crate::commands::connect::{open_session, ConnectResponse};
    use crate::commands::state::AppState;
    use crate::db::{Driver, Engine};

    /// DBUNNY_TEST_SOCKET_URL: a URL whose host is a socket folder, e.g.
    /// postgresql://user:pass@%2Ftmp%2Fpg:5432/db
    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SOCKET_URL"]
    async fn a_unix_socket_opens_a_session() {
        // Docker Desktop can't share a socket with the host, so servers.sh only sets this on Linux
        let Ok(text) = std::env::var("DBUNNY_TEST_SOCKET_URL") else {
            eprintln!("dbunny-skip: a_unix_socket_opens_a_session needs DBUNNY_TEST_SOCKET_URL");
            return;
        };
        let url = url::Url::parse(&text).unwrap();
        let folder = socket_dir(&url).expect("the URL's host must be a socket folder");
        let state = AppState { sessions: Default::default(), saved_connections: Default::default(), encryption_key: [0; 32] };
        // What `open_session` does, keeping the driver to reach its connection
        let driver = PostgresDriver::open(Endpoint { url: url.clone(), ssh: None, known_hosts: None }, None).await.unwrap();
        let session = ConnectResponse::new(state.register(driver.clone()).await.unwrap(), driver.as_ref());
        assert_eq!(session.host, folder, "the socket folder stands for the host");
        let one: i32 = driver.client().await.unwrap().query_one("SELECT 1", &[]).await.unwrap().get(0);
        assert_eq!(one, 1);

        let ssh = SshConfig {
            host: "bastion".into(),
            port: 22,
            user: "u".into(),
            auth_method: "password".into(),
            password: "p".into(),
            key_path: String::new(),
            passphrase: String::new(),
        };
        let err = open_session(&state, Endpoint { url, ssh: Some(ssh), known_hosts: None }, None).await.err().unwrap();
        assert!(err.message().contains("SSH"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_DATABASE_URL"]
    async fn connect_tells_when_the_version_left_its_support() {
        let url = url::Url::parse(&crate::db::postgres::tests_support::test_url()).unwrap();
        let driver = PostgresDriver::open(Endpoint { url, ssh: None, known_hosts: None }, None).await.unwrap();
        let version = crate::db::postgres::tests_support::server_version().await;
        assert_eq!(driver.version(), Some(version));
        let expected = crate::db::version::past_end_of_support(Engine::Postgres, version, chrono::Local::now().date_naive());
        let response = ConnectResponse::new("s".into(), driver.as_ref());
        assert_eq!(response.end_of_support, expected);
        if version.major == 13 {
            assert_eq!(response.end_of_support.as_ref().unwrap().date, "2025-11-13");
        }
        if version.major >= 15 {
            assert_eq!(response.end_of_support, None);
        }
        driver.close().await;
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

#[cfg(test)]
mod version_tests {
    use super::*;
    use crate::db::version::ServerVersion;

    #[test]
    fn postgres_below_13_is_refused() {
        let v = ServerVersion::from_postgres_num;
        assert_eq!(check_postgres_version(v(120022), "12.22").err().unwrap(), "PostgreSQL 12.22 isn't supported; DBunny needs PostgreSQL 13 or newer.");
        assert!(check_postgres_version(v(130000), "13.0").is_ok());
        assert!(check_postgres_version(v(180006), "18.6").is_ok());
    }
}
