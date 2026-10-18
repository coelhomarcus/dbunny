//! Where a MySQL session connects and the `mysql_async` options of its connections.

use std::path::PathBuf;
use std::time::Duration;

use mysql_async::{Opts, OptsBuilder, PoolConstraints, PoolOpts};

use crate::db::net::ssh::SshConfig;
use crate::db::net::tls::TlsSettings;

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
    if endpoint.tls.is_some() {
        // Task 4 maps the SSL settings to `SslOpts`; until then nothing connects without them
        return Err("SSL isn't available for MySQL connections yet".into());
    }
    let (host, port) = match tunnel_port {
        Some(port) => ("127.0.0.1".to_string(), port),
        None => (endpoint.host.clone(), endpoint.port_or_default()),
    };
    let pool = PoolOpts::new().with_constraints(PoolConstraints::new(0, POOL_MAX).expect("0 <= max and max > 0"));
    let builder = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
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

    #[test]
    fn an_empty_database_opens_none() {
        let opts = opts_for(&MysqlEndpoint { database: Some(String::new()), ..endpoint() }, None, ConnRole::Pool).unwrap();
        assert_eq!(opts.db_name(), None);
    }
}
