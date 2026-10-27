//! What the PostgreSQL server tests share: where the test server is and which version it runs.
//! Start a server with `src-tauri/tests/servers.sh up postgres <version>`.

use crate::db::version::{skip_line, ServerVersion};
use crate::db::Engine;

/// The server in `DBUNNY_TEST_DATABASE_URL`.
pub fn test_url() -> String {
    std::env::var("DBUNNY_TEST_DATABASE_URL").expect("set DBUNNY_TEST_DATABASE_URL to run the server tests")
}

/// The test server's version, read once from `server_version_num`.
pub async fn server_version() -> ServerVersion {
    static VERSION: tokio::sync::OnceCell<ServerVersion> = tokio::sync::OnceCell::const_new();
    *VERSION
        .get_or_init(|| async {
            let (client, connection) = tokio_postgres::connect(&test_url(), tokio_postgres::NoTls).await.unwrap();
            let handle = tokio::spawn(connection);
            let num: String = client.query_one("SHOW server_version_num", &[]).await.unwrap().get(0);
            drop(client);
            let _ = handle.await;
            ServerVersion::from_postgres_num(num.parse().unwrap())
        })
        .await
}

/// Whether the test server is PostgreSQL `major.minor` or newer; when it isn't, prints the
/// `dbunny-skip` line for `test` so the caller can return.
pub async fn requires(test: &str, major: u16, minor: u16) -> bool {
    if server_version().await.at_least(major, minor, 0) {
        return true;
    }
    eprintln!("{}", skip_line(test, Engine::Postgres, ServerVersion { major, minor, patch: 0 }));
    false
}
