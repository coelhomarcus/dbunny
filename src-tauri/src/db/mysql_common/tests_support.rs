//! What the MySQL server tests share: where the test server is, a throwaway database per
//! test, and a way to prepare fixtures. Start the servers with `src-tauri/tests/mysql-docker.sh up`.

use mysql_async::prelude::Queryable;

use std::sync::Arc;

use crate::db::mariadb::MariadbDriver;
use crate::db::mysql::MysqlDriver;
use crate::db::mysql_common::connect::{opts_for, ConnRole, MysqlEndpoint};
use crate::db::mysql_common::driver::ServerDriver;

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
    if is_mariadb().await {
        MariadbDriver::open(endpoint, None).await
    } else {
        MysqlDriver::open(endpoint, None).await
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

async fn server_version() -> &'static str {
    static VERSION: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    VERSION.get_or_init(|| async { query_one::<String>(&test_endpoint(), "SELECT VERSION()").await.unwrap_or_default() }).await
}
