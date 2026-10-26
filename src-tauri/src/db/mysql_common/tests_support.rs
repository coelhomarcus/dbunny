//! What the MySQL server tests share: where the test server is, a throwaway database per
//! test, and a way to prepare fixtures. Start a server with `src-tauri/tests/servers.sh up mysql|mariadb <version>`.

use mysql_async::prelude::Queryable;

use std::sync::Arc;

use crate::db::mariadb::MariadbDriver;
use crate::db::mysql::MysqlDriver;
use crate::db::mysql_common::connect::{opts_for, ConnRole, MysqlEndpoint};
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::version::{numbers, skip_line, ServerVersion};
use crate::db::Engine;

/// Tests that change a global setting of the server (like `read_only`) hold this for
/// writing; tests whose checks depend on those settings hold it for reading.
pub static SERVER_SETTINGS: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// Every `TestDb` holds this for reading while it exists; a test that depends on which
/// databases the server has (the first one a session opens) holds it for writing.
static DATABASES: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// The server in `DBUNNY_TEST_MYSQL_URL` (`mysql://user:password@host:port`), without a database.
pub fn test_endpoint() -> MysqlEndpoint {
    endpoint_from("DBUNNY_TEST_MYSQL_URL")
}

/// The server in the environment variable `var`, read like `test_endpoint`.
pub fn endpoint_from(var: &str) -> MysqlEndpoint {
    let text = std::env::var(var).unwrap_or_else(|_| panic!("set {var} to run the MySQL server tests"));
    let url = url::Url::parse(&text).unwrap_or_else(|e| panic!("{var} isn't a URL: {e}"));
    let decode = |text: &str| url::form_urlencoded::parse(format!("x={text}").as_bytes()).next().unwrap().1.into_owned();
    let database = url.path().trim_start_matches('/');
    MysqlEndpoint {
        host: url.host_str().unwrap_or("127.0.0.1").to_string(),
        port: url.port().unwrap_or(3306),
        database: (!database.is_empty()).then(|| decode(database)),
        user: decode(url.username()),
        password: url.password().map(decode).unwrap_or_default(),
        ..Default::default()
    }
}

/// A connection for fixtures: never read-only, whatever the endpoint asks.
pub async fn raw_conn(endpoint: &MysqlEndpoint) -> mysql_async::Conn {
    let endpoint = MysqlEndpoint { read_only: false, ..endpoint.clone() };
    mysql_async::Conn::new(opts_for(&endpoint, None, ConnRole::Editor).unwrap()).await.unwrap()
}

/// Runs `sql` (one statement or several) for a fixture; panics when it fails.
pub async fn exec(endpoint: &MysqlEndpoint, sql: &str) {
    let mut conn = raw_conn(endpoint).await;
    conn.query_drop(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
    let _ = conn.disconnect().await;
}

/// The first column of the first row of `sql`, for checks.
pub async fn query_one<T: mysql_async::prelude::FromRow + Send + 'static>(endpoint: &MysqlEndpoint, sql: &str) -> Option<T> {
    let mut conn = raw_conn(endpoint).await;
    let value = conn.query_first::<T, _>(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
    let _ = conn.disconnect().await;
    value
}

/// A database `dbunny_t_<8 hex>` made for one test. `drop_now` deletes it; a test that
/// fails before that still deletes it when the value is dropped.
pub struct TestDb {
    pub name: String,
    /// The test server with this database open.
    pub endpoint: MysqlEndpoint,
    dropped: bool,
    /// Released after the database is dropped.
    _guard: Box<dyn std::any::Any + Send>,
}

impl TestDb {
    pub async fn new() -> TestDb {
        Self::with_guard(Box::new(DATABASES.read().await)).await
    }

    /// Like `new`, with no other `TestDb` existing while this one does.
    pub async fn alone() -> TestDb {
        Self::with_guard(Box::new(DATABASES.write().await)).await
    }

    async fn with_guard(guard: Box<dyn std::any::Any + Send>) -> TestDb {
        let name = format!("dbunny_t_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let server = test_endpoint();
        exec(&server, &format!("CREATE DATABASE `{name}`")).await;
        let endpoint = MysqlEndpoint { database: Some(name.clone()), ..server };
        TestDb { name, endpoint, dropped: false, _guard: guard }
    }

    pub async fn drop_now(mut self) {
        exec(&test_endpoint(), &format!("DROP DATABASE IF EXISTS `{}`", self.name)).await;
        self.dropped = true;
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if self.dropped {
            return;
        }
        // Not inside the test's runtime, which may be the one unwinding
        let sql = format!("DROP DATABASE IF EXISTS `{}`", self.name);
        let _ = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(exec(&test_endpoint(), &sql));
        })
        .join();
    }
}

/// Opens a session on `endpoint` with the driver of the flavor the test server (the one in
/// `DBUNNY_TEST_MYSQL_URL`) is.
pub async fn open_server(endpoint: MysqlEndpoint) -> Result<Arc<ServerDriver>, String> {
    open_server_trusting(endpoint, None).await
}

/// `open_server` with the SSH host key fingerprint the person confirmed.
pub async fn open_server_trusting(endpoint: MysqlEndpoint, trust_host_key: Option<String>) -> Result<Arc<ServerDriver>, String> {
    let mariadb = is_mariadb().await;
    MARIADB_NOW.store(mariadb, std::sync::atomic::Ordering::Relaxed);
    if mariadb {
        MariadbDriver::open(endpoint, trust_host_key).await
    } else {
        MysqlDriver::open(endpoint, trust_host_key).await
    }
}

/// Whether the test server is MariaDB; set by `open_server`, which every server test calls first.
static MARIADB_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// For checks that read the same on both servers but for how they write a type or a rule.
pub fn on_mariadb() -> bool {
    MARIADB_NOW.load(std::sync::atomic::Ordering::Relaxed)
}

/// The name of the test server's engine as the app's messages spell it.
pub fn label() -> &'static str {
    if on_mariadb() { "MariaDB" } else { "MySQL" }
}

/// How the server writes an integer type: MariaDB keeps the display width.
pub fn ty(base: &str) -> String {
    match (on_mariadb(), base) {
        (true, "int") => "int(11)".into(),
        (true, "bigint") => "bigint(20)".into(),
        _ => base.into(),
    }
}

/// Whether the test server is MySQL (and not MariaDB), read once from `VERSION()`.
pub async fn is_mysql() -> bool {
    !server_version().await.to_ascii_lowercase().contains("mariadb")
}

/// Whether the test server is MariaDB.
pub async fn is_mariadb() -> bool {
    !is_mysql().await
}

/// The test server's version, without the `5.5.5-` prefix older MariaDB servers send.
pub async fn version() -> ServerVersion {
    let text = server_version().await;
    let real = text.strip_prefix("5.5.5-").unwrap_or(text);
    numbers(real).map(|(version, _)| version).unwrap_or_else(|| panic!("couldn't read the version in \"{text}\""))
}

/// Whether the test server is `engine` at `major.minor.patch` or newer. A server of the other
/// family answers false quietly; an older one prints the `dbunny-skip` line for `test`.
pub async fn requires(test: &str, engine: Engine, major: u16, minor: u16, patch: u16) -> bool {
    let family = if is_mariadb().await { Engine::Mariadb } else { Engine::Mysql };
    if family != engine {
        return false;
    }
    if version().await.at_least(major, minor, patch) {
        return true;
    }
    eprintln!("{}", skip_line(test, engine, ServerVersion { major, minor, patch }));
    false
}

async fn server_version() -> &'static str {
    static VERSION: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    VERSION.get_or_init(|| async { query_one::<String>(&test_endpoint(), "SELECT VERSION()").await.unwrap_or_default() }).await
}

// TLS and SSH: the certificates in DBUNNY_TEST_MYSQL_SSL_DIR belong to the server servers.sh
// started, and the SSH target is its container name.

pub fn ssl_file(name: &str) -> String {
    let dir = std::env::var("DBUNNY_TEST_MYSQL_SSL_DIR").expect("set DBUNNY_TEST_MYSQL_SSL_DIR to run the MySQL TLS tests");
    format!("{}/{name}", dir.trim_end_matches('/'))
}

pub fn with_tls(endpoint: MysqlEndpoint, mode: &str, root: &str, cert: &str, key: &str) -> MysqlEndpoint {
    let file = |name: &str| if name.is_empty() { String::new() } else { ssl_file(name) };
    MysqlEndpoint {
        tls: Some(crate::db::net::tls::TlsSettings { mode: mode.into(), root_cert: file(root), cert: file(cert), key: file(key) }),
        ..endpoint
    }
}

/// What `SHOW STATUS LIKE 'Ssl_cipher'` says on the editor's and on a pool connection.
pub async fn ciphers(driver: &ServerDriver) -> (String, String) {
    let in_editor: (String, String) = driver
        .editor()
        .run(|c| Box::pin(async move { c.query_first("SHOW STATUS LIKE 'Ssl_cipher'").await.map(Option::unwrap).map_err(|e| e.to_string()) }))
        .await
        .unwrap();
    let in_pool: (String, String) = driver.conn().await.unwrap().query_first("SHOW STATUS LIKE 'Ssl_cipher'").await.unwrap().unwrap();
    (in_editor.1, in_pool.1)
}

/// A user that must log in with a client certificate; dropped with the value, also when
/// the test fails.
pub struct CertUser {
    pub name: String,
}

impl CertUser {
    pub async fn new() -> CertUser {
        let name = format!("dbunny_t_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        exec(&test_endpoint(), &format!("CREATE USER '{name}'@'%' IDENTIFIED BY 'certpass' REQUIRE X509")).await;
        CertUser { name }
    }
}

impl Drop for CertUser {
    fn drop(&mut self) {
        let sql = format!("DROP USER IF EXISTS '{}'@'%'", self.name);
        let _ = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(exec(&test_endpoint(), &sql));
        })
        .join();
    }
}

pub struct SshEnv {
    pub ssh: crate::db::net::ssh::SshConfig,
    pub target: (String, u16),
}

pub fn ssh_env() -> SshEnv {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run the SSH tests"));
    let ssh = url::Url::parse(&format!("ssh://{}", var("DBUNNY_TEST_MYSQL_SSH"))).unwrap();
    let target = var("DBUNNY_TEST_MYSQL_SSH_TARGET");
    let (host, port) = target.rsplit_once(':').unwrap();
    SshEnv {
        ssh: crate::db::net::ssh::SshConfig {
            host: ssh.host_str().unwrap().into(),
            port: ssh.port().unwrap_or(22),
            user: ssh.username().into(),
            auth_method: "password".into(),
            password: ssh.password().unwrap_or("").into(),
            ..Default::default()
        },
        target: (host.into(), port.parse().unwrap()),
    }
}

/// Opens a session through the SSH server, confirming its host key the way the UI does:
/// the first attempt is refused with the fingerprint, the second passes it.
pub async fn open_through_ssh(endpoint: MysqlEndpoint) -> Result<Arc<ServerDriver>, String> {
    let known_hosts = std::env::temp_dir().join(format!("dbunny-mysql-ssh-test-{}.json", uuid::Uuid::new_v4()));
    let endpoint = MysqlEndpoint { known_hosts: Some(known_hosts.clone()), ..endpoint };
    let err = match open_server(endpoint.clone()).await {
        Ok(_) => panic!("an unknown host key must not be accepted silently"),
        Err(e) => e,
    };
    let problem: serde_json::Value = serde_json::from_str(&err).unwrap_or_else(|_| panic!("not a host key error: {err}"));
    assert_eq!(problem["sshHostKey"]["status"], "unknown");
    let fingerprint = problem["sshHostKey"]["fingerprint"].as_str().unwrap().to_string();
    let result = open_server_trusting(endpoint, Some(fingerprint)).await;
    let _ = std::fs::remove_file(known_hosts);
    result
}

pub fn through_ssh(db: &TestDb) -> MysqlEndpoint {
    let env = ssh_env();
    MysqlEndpoint {
        // The container's name on the Docker network: only the SSH server can resolve it
        host: env.target.0,
        port: env.target.1,
        ssh: Some(env.ssh),
        ..db.endpoint.clone()
    }
}
