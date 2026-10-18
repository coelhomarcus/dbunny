//! Where a MySQL session connects and the `mysql_async` options of its connections.

use std::path::PathBuf;
use std::time::Duration;

use mysql_async::{ClientIdentity, Opts, OptsBuilder, PoolConstraints, PoolOpts, SslOpts};

use crate::db::net::ssh::SshConfig;
use crate::db::net::tls::{read_file, TlsSettings};

/// What a session is opened with.
#[derive(Clone, Debug, Default)]
pub struct MysqlEndpoint {
    /// A host name, or the path of a Unix socket when it starts with "/".
    pub host: String,
    /// 0 means 3306.
    pub port: u16,
    pub database: Option<String>,
    pub user: String,
    pub password: String,
    /// None: no TLS.
    pub tls: Option<TlsSettings>,
    pub ssh: Option<SshConfig>,
    /// The app's SSH known hosts file, when connecting through SSH.
    pub known_hosts: Option<PathBuf>,
    /// Every connection only reads.
    pub read_only: bool,
}

impl MysqlEndpoint {
    /// The port, with 3306 for 0.
    pub fn port_or_default(&self) -> u16 {
        if self.port == 0 {
            3306
        } else {
            self.port
        }
    }
}

/// Which connection of a session the options are for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnRole {
    /// The pool behind the explorer, table views, structure and administration.
    Pool,
    /// The query editor's own connection.
    Editor,
}

/// Most connections the pool of one session opens.
const POOL_MAX: usize = 4;

/// The `mysql_async` options for one kind of connection of `endpoint`. `tunnel_port` is the
/// local port of the SSH tunnel the connection goes through, when there is one.
///
/// - `CLIENT_FOUND_ROWS` always: without it MySQL counts only the rows an UPDATE changed,
///   and saving a cell with the value it already has would look like a missing row.
/// - The statements of `init_statements` run on every new connection, and again after the
///   pool resets one it got back (`COM_RESET_CONNECTION` clears the session variables).
/// - The pool opens at most 4 connections.
pub(crate) fn opts_for(endpoint: &MysqlEndpoint, tunnel_port: Option<u16>, role: ConnRole) -> Result<Opts, String> {
    let socket = is_socket(&endpoint.host);
    if socket && cfg!(windows) {
        return Err("Unix sockets aren't available on Windows: use the host name and port".into());
    }
    // A socket is a local file: no tunnel and no TLS
    let tunnel_port = tunnel_port.filter(|_| !socket);
    let (host, port) = match tunnel_port {
        Some(port) => ("127.0.0.1".to_string(), port),
        None => (endpoint.host.clone(), endpoint.port_or_default()),
    };
    let ssl = match (&endpoint.tls, socket) {
        (Some(tls), false) => Some(ssl_opts(tls, &endpoint.host, tunnel_port.is_some())?),
        _ => None,
    };
    let pool = PoolOpts::new().with_constraints(PoolConstraints::new(0, POOL_MAX).expect("0 <= max and max > 0"));
    let builder = OptsBuilder::default()
        .ip_or_hostname(if socket { "localhost".to_string() } else { host })
        .tcp_port(port)
        .socket(socket.then(|| endpoint.host.clone()))
        .ssl_opts(ssl)
        .user(Some(endpoint.user.clone()))
        .pass(Some(endpoint.password.clone()))
        .db_name(endpoint.database.clone().filter(|name| !name.is_empty()))
        .client_found_rows(true)
        .setup(init_statements(endpoint.read_only, role))
        // Never switch to the socket the server names: through a tunnel or a container port,
        // that path on this machine may belong to another server
        .prefer_socket(false)
        // Notice a dead connection (sleep, network change) within minutes instead of hours
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .pool_opts(pool);
    Ok(builder.into())
}

/// Whether `host` is the path of a Unix socket rather than a host name.
pub(crate) fn is_socket(host: &str) -> bool {
    host.starts_with('/')
}

/// The TLS options of the SSL mode `tls`, which are the same on every connection:
/// - `require` encrypts and accepts any certificate (servers make their own);
/// - `verify-ca` checks the certificate chain against the CA file (the built-in roots when
///   there is none) but not the server's name. That is what is asked of the library; but
///   `mysql_async` 0.37.1 skips the name error only when rustls' text contains
///   "NotValidForName", and rustls now reports `NotValidForNameContext`. Until that is
///   fixed this mode also refuses a certificate that doesn't name the host (never weaker
///   than asked), and `errors.rs` says so;
/// - `verify-full` checks the name too. Through an SSH tunnel the connection goes to
///   127.0.0.1, so the name checked stays the original `host`.
///
/// A client certificate and key (PEM) are sent when both are set.
fn ssl_opts(tls: &TlsSettings, host: &str, tunnelled: bool) -> Result<SslOpts, String> {
    let mut ssl = match tls.mode.as_str() {
        "require" => SslOpts::default().with_danger_accept_invalid_certs(true),
        "verify-ca" => SslOpts::default().with_danger_skip_domain_validation(true),
        "verify-full" if tunnelled => SslOpts::default().with_danger_tls_hostname_override(Some(host.to_string())),
        "verify-full" => SslOpts::default(),
        other => return Err(format!("Invalid SSL mode \"{other}\". Use require, verify-ca or verify-full")),
    };
    if tls.mode != "require" && !tls.root_cert.trim().is_empty() {
        ssl = ssl.with_root_certs(vec![read_file("CA certificate", tls.root_cert.trim())?.into()]);
    }
    match (tls.cert.trim(), tls.key.trim()) {
        ("", "") => {}
        ("", _) | (_, "") => return Err("A client certificate needs both the certificate and its key".into()),
        (cert, key) => {
            let identity = ClientIdentity::new(read_file("client certificate", cert)?.into(), read_file("client key", key)?.into());
            ssl = ssl.with_client_identity(Some(identity));
        }
    }
    Ok(ssl)
}

/// What every new connection runs first: utf8mb4, UTC for `TIMESTAMP` values, read-only
/// transactions when asked, and on the pool a short wait for row locks (a save blocked by
/// the editor's transaction fails in seconds instead of 50).
pub(crate) fn init_statements(read_only: bool, role: ConnRole) -> Vec<String> {
    let mut statements = vec!["SET NAMES utf8mb4".to_string(), "SET time_zone = '+00:00'".to_string()];
    if read_only {
        statements.push("SET SESSION TRANSACTION READ ONLY".into());
    }
    if role == ConnRole::Pool {
        statements.push("SET SESSION innodb_lock_wait_timeout = 5".into());
    }
    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> MysqlEndpoint {
        MysqlEndpoint {
            host: "db.example.com".into(),
            port: 0,
            database: Some("shop".into()),
            user: "app".into(),
            password: "p@ss".into(),
            ..Default::default()
        }
    }

    #[test]
    fn options_carry_the_endpoint_and_the_session_settings() {
        let opts = opts_for(&endpoint(), None, ConnRole::Pool).unwrap();
        assert_eq!(opts.ip_or_hostname(), "db.example.com");
        assert_eq!(opts.tcp_port(), 3306);
        assert_eq!((opts.user(), opts.pass(), opts.db_name()), (Some("app"), Some("p@ss"), Some("shop")));
        assert!(opts.client_found_rows());
        assert!(!opts.prefer_socket());
        assert_eq!(opts.setup(), init_statements(false, ConnRole::Pool).as_slice());
        assert_eq!(opts.pool_opts().constraints().max(), 4);
    }

    #[test]
    fn a_tunnel_connects_to_its_local_port() {
        let opts = opts_for(&endpoint(), Some(51234), ConnRole::Editor).unwrap();
        assert_eq!((opts.ip_or_hostname(), opts.tcp_port()), ("127.0.0.1", 51234));
        assert!(opts.client_found_rows());
        assert_eq!(opts.setup(), init_statements(false, ConnRole::Editor).as_slice());
    }

    #[test]
    fn read_only_sessions_start_read_only_transactions_on_every_connection() {
        let read_only = MysqlEndpoint { read_only: true, ..endpoint() };
        for role in [ConnRole::Pool, ConnRole::Editor] {
            let opts = opts_for(&read_only, None, role).unwrap();
            assert!(opts.setup().contains(&"SET SESSION TRANSACTION READ ONLY".to_string()), "{role:?}");
        }
        assert!(!init_statements(true, ConnRole::Editor).contains(&"SET SESSION innodb_lock_wait_timeout = 5".to_string()));
    }

    fn tls(mode: &str) -> Option<TlsSettings> {
        Some(TlsSettings { mode: mode.into(), ..Default::default() })
    }

    /// A file with `content` in the temp folder, deleted by the caller.
    fn temp_file(content: &str) -> String {
        let path = std::env::temp_dir().join(format!("dbunny-mysql-tls-{}.pem", uuid::Uuid::new_v4()));
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn ssl_modes_map_to_tls_options() {
        let ca = temp_file("the ca");
        let with = |mode: &str| MysqlEndpoint {
            tls: Some(TlsSettings { mode: mode.into(), root_cert: ca.clone(), ..Default::default() }),
            ..endpoint()
        };

        let require = opts_for(&with("require"), None, ConnRole::Pool).unwrap();
        let ssl = require.ssl_opts().expect("require turns TLS on");
        assert!(ssl.accept_invalid_certs());
        assert!(ssl.client_identity().is_none());

        let verify_ca = opts_for(&with("verify-ca"), None, ConnRole::Pool).unwrap();
        let ssl = verify_ca.ssl_opts().unwrap();
        assert!(!ssl.accept_invalid_certs());
        assert!(ssl.skip_domain_validation());
        assert_eq!(ssl.root_certs().len(), 1);
        assert_eq!(&*ssl.root_certs()[0].read().await.unwrap(), b"the ca");

        let verify_full = opts_for(&with("verify-full"), None, ConnRole::Editor).unwrap();
        let ssl = verify_full.ssl_opts().unwrap();
        assert!(!ssl.accept_invalid_certs());
        assert!(!ssl.skip_domain_validation());
        assert_eq!(ssl.root_certs().len(), 1);
        assert_eq!(ssl.tls_hostname_override(), None, "without a tunnel the host itself is validated");

        let err = opts_for(&with("nope"), None, ConnRole::Pool).err().unwrap();
        assert_eq!(err, "Invalid SSL mode \"nope\". Use require, verify-ca or verify-full");

        // Without a CA file the trusted roots are the built-in ones
        let no_ca = MysqlEndpoint { tls: tls("verify-ca"), ..endpoint() };
        assert!(opts_for(&no_ca, None, ConnRole::Pool).unwrap().ssl_opts().unwrap().root_certs().is_empty());

        // A CA file that can't be read says which
        let missing = MysqlEndpoint { tls: Some(TlsSettings { mode: "verify-ca".into(), root_cert: "/nope/ca.pem".into(), ..Default::default() }), ..endpoint() };
        let err = opts_for(&missing, None, ConnRole::Pool).err().unwrap();
        assert!(err.starts_with("Couldn't read the CA certificate /nope/ca.pem"), "{err}");
        std::fs::remove_file(ca).unwrap();
    }

    #[tokio::test]
    async fn client_certificates_need_both_files() {
        let cert = temp_file("the cert");
        let key = temp_file("the key");
        let settings = |cert: &str, key: &str| MysqlEndpoint {
            tls: Some(TlsSettings { mode: "require".into(), cert: cert.into(), key: key.into(), ..Default::default() }),
            ..endpoint()
        };

        let opts = opts_for(&settings(&cert, &key), None, ConnRole::Pool).unwrap();
        let identity = opts.ssl_opts().unwrap().client_identity().expect("the identity is set");
        assert_eq!(&*identity.cert_chain().read().await.unwrap(), b"the cert");
        assert_eq!(&*identity.priv_key().read().await.unwrap(), b"the key");

        for (cert, key) in [(cert.as_str(), ""), ("", key.as_str())] {
            let err = opts_for(&settings(cert, key), None, ConnRole::Pool).err().unwrap();
            assert_eq!(err, "A client certificate needs both the certificate and its key");
        }
        let err = opts_for(&settings(&cert, "/nope/key.pem"), None, ConnRole::Pool).err().unwrap();
        assert!(err.starts_with("Couldn't read the client key /nope/key.pem"), "{err}");
        std::fs::remove_file(cert).unwrap();
        std::fs::remove_file(key).unwrap();
    }

    #[test]
    fn a_tunnel_keeps_the_original_host_for_certificate_names() {
        let ca = temp_file("the ca");
        let with = |mode: &str| MysqlEndpoint {
            tls: Some(TlsSettings { mode: mode.into(), root_cert: ca.clone(), ..Default::default() }),
            ..endpoint()
        };
        let full = opts_for(&with("verify-full"), Some(51234), ConnRole::Pool).unwrap();
        assert_eq!((full.ip_or_hostname(), full.tcp_port()), ("127.0.0.1", 51234));
        assert_eq!(full.ssl_opts().unwrap().tls_hostname_override(), Some("db.example.com"));
        for mode in ["require", "verify-ca"] {
            let opts = opts_for(&with(mode), Some(51234), ConnRole::Pool).unwrap();
            assert_eq!(opts.ssl_opts().unwrap().tls_hostname_override(), None, "{mode}");
            assert_eq!(opts.ip_or_hostname(), "127.0.0.1", "{mode}");
        }
        std::fs::remove_file(ca).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_path_connects_without_tls_or_tunnel() {
        let socket = MysqlEndpoint {
            host: "/var/run/mysqld/mysqld.sock".into(),
            tls: tls("verify-full"),
            ..endpoint()
        };
        // Neither the TLS settings nor a tunnel port apply to a local file
        let opts = opts_for(&socket, Some(51234), ConnRole::Pool).unwrap();
        assert_eq!(opts.socket(), Some("/var/run/mysqld/mysqld.sock"));
        assert!(opts.ssl_opts().is_none());
        assert_eq!((opts.user(), opts.db_name()), (Some("app"), Some("shop")));
        assert!(opts.client_found_rows());
        assert_eq!(opts.setup(), init_statements(false, ConnRole::Pool).as_slice());

        // An ordinary host never sets a socket
        assert_eq!(opts_for(&endpoint(), None, ConnRole::Pool).unwrap().socket(), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_socket_path_is_refused_on_windows() {
        let socket = MysqlEndpoint { host: "/var/run/mysqld/mysqld.sock".into(), ..endpoint() };
        let err = opts_for(&socket, None, ConnRole::Pool).err().unwrap();
        assert_eq!(err, "Unix sockets aren't available on Windows: use the host name and port");
    }

    #[test]
    fn an_empty_database_opens_none() {
        let opts = opts_for(&MysqlEndpoint { database: Some(String::new()), ..endpoint() }, None, ConnRole::Pool).unwrap();
        assert_eq!(opts.db_name(), None);
    }
}
