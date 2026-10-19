//! What is MySQL's own: its version rules, capabilities and system databases.

use async_trait::async_trait;
use mysql_async::prelude::Queryable;

use super::version::parse_mysql_version;
use crate::db::model::ExplainResult;
use crate::db::mysql_common::driver::ServerDriver;
use crate::db::mysql_common::errors::error_message;
use crate::db::mysql_common::flavor::Flavor;
use crate::db::mysql_common::plan::parse_tree;
use crate::db::mysql_common::version::ServerVersion;
use crate::db::{Capabilities, Engine, ExplainFormat};

pub struct MysqlFlavor;

#[async_trait]
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

    /// `EXPLAIN FORMAT=TREE`, which doesn't run the statement, or `EXPLAIN ANALYZE`, which does.
    async fn explain(&self, driver: &ServerDriver, statement: &str, analyze: bool) -> Result<ExplainResult, String> {
        let explain_sql = if analyze { format!("EXPLAIN ANALYZE {statement}") } else { format!("EXPLAIN FORMAT=TREE {statement}") };
        let endpoint = driver.endpoint().clone();
        let text = driver
            .editor()
            .run(move |conn| {
                Box::pin(async move { conn.query_first::<String, _>(explain_sql).await.map_err(|e| error_message(&e, &endpoint)) })
            })
            .await?
            .ok_or("The server returned no plan")?;
        Ok(ExplainResult { plan: parse_tree(&text)?, analyzed: analyze, format: ExplainFormat::MysqlTree })
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

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_MYSQL_URL"]
    async fn explain_hook_is_reached_after_the_shared_checks() {
        use crate::db::mysql_common::tests_support::{is_mysql, open_server, TestDb};
        use crate::db::Driver;
        if !is_mysql().await {
            return;
        }
        let db = TestDb::new().await;
        let driver = open_server(db.endpoint.clone()).await.unwrap();
        let console = driver.console().unwrap();
        assert_eq!(console.explain("EXPLAIN SELECT 1", false).await.err().unwrap(), "Leave out EXPLAIN: the plan is requested for you.");
        let explained = driver.flavor().explain(&driver, "SELECT 1", false).await.unwrap();
        assert_eq!(explained.format, ExplainFormat::MysqlTree);
        assert!(!explained.analyzed);
        driver.close().await;
        db.drop_now().await;
    }
}
