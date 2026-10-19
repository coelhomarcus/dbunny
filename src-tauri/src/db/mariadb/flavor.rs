//! What is MariaDB's own: its version rules, capabilities and system databases.

use async_trait::async_trait;

use super::version::parse_mariadb_version;
use crate::db::model::{ExplainResult, RoleInfo};
use crate::db::mysql_common::admin::RawSession;
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::mysql_common::flavor::Flavor;
use crate::db::mysql_common::version::ServerVersion;
use crate::db::{unsupported, Capabilities, Engine};

pub struct MariadbFlavor;

/// Until the pieces that read the server's accounts and sessions exist for MariaDB.
fn not_yet(what: &str) -> mysql_async::Error {
    mysql_async::Error::Other(unsupported(Engine::Mariadb, what).into())
}

#[async_trait]
impl Flavor for MariadbFlavor {
    fn engine(&self) -> Engine {
        Engine::Mariadb
    }

    fn parse_version(&self, text: &str) -> Result<ServerVersion, String> {
        parse_mariadb_version(text)
    }

    fn capabilities(&self, version: ServerVersion) -> Capabilities {
        Capabilities::mariadb(version)
    }

    fn system_databases(&self) -> &'static [&'static str] {
        &["mysql", "sys", "information_schema", "performance_schema"]
    }

    /// MariaDB has no `super_read_only`.
    fn read_only_sql(&self) -> &'static str {
        "SELECT @@read_only, 0"
    }

    async fn explain(&self, _driver: &ServerDriver, _statement: &str, _analyze: bool) -> Result<ExplainResult, String> {
        Err(unsupported(Engine::Mariadb, "EXPLAIN"))
    }

    async fn accounts(&self, _conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>> {
        Err(not_yet("Roles"))
    }

    async fn own_account(&self, _conn: &mut mysql_async::Conn) -> mysql_async::Result<RoleInfo> {
        Err(not_yet("Roles"))
    }

    async fn rich_sessions(&self, _conn: &mut mysql_async::Conn, _database: Option<&str>) -> mysql_async::Result<Vec<RawSession>> {
        Err(not_yet("Activity"))
    }

    async fn blockers(&self, _conn: &mut mysql_async::Conn) -> Vec<(i64, i64)> {
        Vec::new()
    }

    async fn session_ids_in(&self, _conn: &mut mysql_async::Conn, _name: &str) -> mysql_async::Result<Vec<i64>> {
        Err(not_yet("Activity"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mariadb_flavor_answers_as_mariadb() {
        let flavor = MariadbFlavor;
        assert_eq!(flavor.engine(), Engine::Mariadb);
        let v = ServerVersion { major: 10, minor: 6, patch: 0 };
        assert_eq!(flavor.capabilities(v), Capabilities::mariadb(v));
        assert_eq!(flavor.system_databases(), ["mysql", "sys", "information_schema", "performance_schema"]);
        assert_eq!(flavor.parse_version("11.4.13-MariaDB-ubu2404").unwrap().minor, 4);
    }
}
