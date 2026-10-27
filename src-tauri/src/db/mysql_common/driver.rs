//! The session on a server of the MySQL family: a pool for the explorer, table views,
//! structure and administration, and the query editor's own connection.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::Pool;

use super::connect::{is_socket, opts_for, ConnRole, MysqlEndpoint};
use super::errors::error_message;
use super::flavor::Flavor;
use super::version::ServerVersion;
use super::editor::EditorConn;
use crate::db::version::mysql_is_stock;
use crate::db::net::ssh::{open_tunnel, KnownHosts, Tunnel};
use crate::db::{Capabilities, Catalog, Console, ConnectionInfo, Data, Driver, Engine, ReadOnlyQueries, SchemaEditor, ServerAdmin, Transactions};

/// How long `close` waits for the editor's query to be killed.
const KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// One open session on a server of the MySQL family, whose `Flavor` says where its
/// flavor differs.
pub struct ServerDriver {
    /// The connections of the explorer, table views, structure and administration; None
    /// once the session was closed.
    pool: Mutex<Option<Pool>>,
    /// The query editor's connection, which also keeps its transaction.
    editor: EditorConn,
    /// Where the session connects, with the database it opened.
    endpoint: MysqlEndpoint,
    /// The SSH tunnel the connections go through; held to keep it open, and closed with
    /// the session.
    tunnel: Mutex<Option<Arc<Tunnel>>>,
    info: ConnectionInfo,
    #[allow(dead_code)] // read through `version()`
    version: ServerVersion,
    /// MySQL or MariaDB itself, not a compatible server (Aurora, TiDB, Vitess) whose version
    /// follows its own vendor's support.
    stock: bool,
    capabilities: Capabilities,
    flavor: &'static dyn Flavor,
}

impl ServerDriver {
    /// Connects to `endpoint`: the tunnel, the editor's connection, the version check, the
    /// database to open when none was asked for, whether the server only reads, and the
    /// pool. `trust_host_key` is an SSH host key fingerprint the user just confirmed.
    pub async fn open(endpoint: MysqlEndpoint, trust_host_key: Option<String>, flavor: &'static dyn Flavor) -> Result<Arc<ServerDriver>, String> {
        let mut endpoint = endpoint;
        endpoint.engine = Some(flavor.engine());
        if is_socket(&endpoint.host) && endpoint.ssh.is_some() {
            return Err("A socket is a local file, so it can't be reached through an SSH tunnel".into());
        }
        let tunnel = match (&endpoint.ssh, &endpoint.known_hosts) {
            (Some(ssh), Some(known_hosts)) => {
                let target = (endpoint.host.clone(), endpoint.port_or_default());
                Some(Arc::new(open_tunnel(ssh, target, KnownHosts::new(known_hosts.clone()), trust_host_key).await?))
            }
            _ => None,
        };
        let tunnel_port = tunnel.as_ref().map(|t| t.local_port);

        let editor = EditorConn::open(opts_for(&endpoint, tunnel_port, ConnRole::Editor)?, endpoint.clone()).await?;
        let shown = endpoint.clone();
        let fail = move |e: mysql_async::Error| error_message(&e, &shown);

        let fail_version = fail.clone();
        let text: String = editor
            .run(|c| Box::pin(async move { c.query_first("SELECT VERSION()").await.map(Option::unwrap_or_default).map_err(fail_version) }))
            .await?;
        let version = flavor.parse_version(&text)?;
        // Aurora MySQL reports a plain 8.0.x; only it answers @@aurora_version
        let aurora = flavor.engine() == Engine::Mysql
            && editor.run(|c| Box::pin(async move { c.query_drop("SELECT @@aurora_version").await.map_err(|e| e.to_string()) })).await.is_ok();
        let stock = flavor.engine() != Engine::Mysql || mysql_is_stock(&text, aurora);

        if endpoint.database.as_deref().unwrap_or("").is_empty() {
            let fail_first = fail.clone();
            let system = flavor.system_databases();
            let placeholders = vec!["?"; system.len()].join(", ");
            let first: Option<String> = editor
                .run(|c| {
                    Box::pin(async move {
                        c.exec_first(
                            format!(
                                "SELECT schema_name FROM information_schema.schemata \
                                 WHERE schema_name NOT IN ({placeholders}) ORDER BY schema_name LIMIT 1"
                            ),
                            system.to_vec(),
                        )
                        .await
                        .map_err(fail_first)
                    })
                })
                .await?;
            endpoint.database = first;
            if let Some(name) = endpoint.database.clone() {
                let fail_use = fail.clone();
                let sql = format!("USE `{}`", name.replace('`', "``"));
                editor.run(|c| Box::pin(async move { c.query_drop(sql).await.map_err(fail_use) })).await?;
                editor.set_opts(opts_for(&endpoint, tunnel_port, ConnRole::Editor)?);
            }
        }

        let fail_flags = fail.clone();
        let read_only_sql = flavor.read_only_sql();
        let (server_read_only, super_read_only): (i64, i64) = editor
            .run(|c| {
                Box::pin(async move {
                    c.query_first(read_only_sql).await.map(Option::unwrap_or_default).map_err(fail_flags)
                })
            })
            .await?;

        let pool = Pool::new(opts_for(&endpoint, tunnel_port, ConnRole::Pool)?);
        let info = ConnectionInfo {
            database: endpoint.database.clone().unwrap_or_default(),
            host: endpoint.host.clone(),
            port: endpoint.port_or_default(),
            server_version: version.to_string(),
            read_only: endpoint.read_only || server_read_only != 0 || super_read_only != 0,
        };
        Ok(Arc::new(ServerDriver {
            pool: Mutex::new(Some(pool)),
            editor,
            endpoint,
            tunnel: Mutex::new(tunnel),
            info,
            version,
            stock,
            capabilities: flavor.capabilities(version),
            flavor,
        }))
    }

    /// A connection from the pool; it goes back when dropped.
    pub(crate) async fn conn(&self) -> Result<mysql_async::Conn, String> {
        let pool = lock(&self.pool).clone().ok_or_else(|| "Session not found".to_string())?;
        pool.get_conn().await.map_err(|e| error_message(&e, &self.endpoint))
    }

    /// The query editor's connection.
    pub(crate) fn editor(&self) -> &EditorConn {
        &self.editor
    }

    /// The database the session opened, or "" when the server had none to open.
    pub(crate) fn database(&self) -> &str {
        self.endpoint.database.as_deref().unwrap_or("")
    }

    /// Where the session connects, with the database it opened.
    pub fn endpoint(&self) -> &MysqlEndpoint {
        &self.endpoint
    }

    /// The flavor of the server this session talks to.
    pub fn flavor(&self) -> &'static dyn Flavor {
        self.flavor
    }

    /// The server's version, for what changes with it (CHECK constraints, EXPLAIN ANALYZE).
    pub fn version(&self) -> ServerVersion {
        self.version
    }
}

#[async_trait]
impl Driver for ServerDriver {
    fn engine(&self) -> Engine {
        self.flavor.engine()
    }

    /// Only for MySQL and MariaDB themselves: the end of support follows their lines.
    fn version(&self) -> Option<ServerVersion> {
        self.stock.then(|| ServerDriver::version(self))
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn info(&self) -> ConnectionInfo {
        self.info.clone()
    }

    /// Lets go of the editor's connection and kills it on the server, so a query it is
    /// running stops there too (waiting at most 2 s for that), then shuts the pool down and
    /// closes the tunnel. Anything the session is asked afterwards answers "Session not found".
    async fn close(&self) {
        // In this order: a command that holds the editor's connection either sees the session
        // closed before it starts, or runs on the connection this id names
        self.editor.close();
        let editor_id = self.editor.connection_id();
        let pool = lock(&self.pool).take();
        if let Some(pool) = pool {
            let kill = async {
                let mut conn = pool.get_conn().await?;
                conn.query_drop(format!("KILL CONNECTION {editor_id}")).await
            };
            // The best it can do: the editor may already be gone, or the server unreachable
            let _ = tokio::time::timeout(KILL_TIMEOUT, kill).await;
            // Waits for the connections commands still hold, so it isn't waited for here
            tokio::spawn(async move {
                let _ = pool.disconnect().await;
            });
        }
        let tunnel = lock(&self.tunnel).take();
        drop(tunnel);
    }

    fn catalog(&self) -> &dyn Catalog {
        self
    }

    fn data(&self) -> &dyn Data {
        self
    }

    fn console(&self) -> Option<&dyn Console> {
        Some(self)
    }

    fn transactions(&self) -> Option<&dyn Transactions> {
        Some(self)
    }

    fn schema_editor(&self) -> Option<&dyn SchemaEditor> {
        Some(self)
    }

    fn admin(&self) -> Option<&dyn ServerAdmin> {
        Some(self)
    }

    fn reader(&self) -> Option<&dyn ReadOnlyQueries> {
        Some(self)
    }
}

/// A mutex a panicking task left poisoned still guards a usable value.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
