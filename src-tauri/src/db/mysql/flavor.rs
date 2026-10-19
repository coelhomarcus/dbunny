//! What is MySQL's own: its version rules, capabilities and system databases.

use super::version::parse_mysql_version;
use crate::db::mysql_common::flavor::Flavor;
use crate::db::mysql_common::version::ServerVersion;
use crate::db::{Capabilities, Engine};

pub struct MysqlFlavor;

impl Flavor for MysqlFlavor {
    fn engine(&self) -> Engine {
        Engine::Mysql
    }

    fn parse_version(&self, text: &str) -> Result<ServerVersion, String> {
        parse_mysql_version(text)
    }

    fn capabilities(&self, version: ServerVersion) -> Capabilities {
        Capabilities::mysql(version)
    }

    fn system_databases(&self) -> &'static [&'static str] {
        &["mysql", "sys", "information_schema", "performance_schema"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_flavor_answers_as_mysql() {
        let flavor = MysqlFlavor;
        assert_eq!(flavor.engine(), Engine::Mysql);
        let v = |patch| ServerVersion { major: 8, minor: 0, patch };
        assert!(!flavor.capabilities(v(17)).explain_analyze);
        assert!(flavor.capabilities(v(18)).explain_analyze);
        assert_eq!(flavor.system_databases(), ["mysql", "sys", "information_schema", "performance_schema"]);
        assert_eq!(flavor.parse_version("8.4.11").unwrap().minor, 4);
    }
}
