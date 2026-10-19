//! What differs from one server of the MySQL family to another. The shared driver asks its
//! `Flavor` where the servers disagree and does everything else itself.

use async_trait::async_trait;

use super::driver::ServerDriver;
use super::version::ServerVersion;
use crate::db::model::ExplainResult;
use crate::db::{Capabilities, Engine};

#[async_trait]
pub trait Flavor: Send + Sync {
    /// The engine sessions of this flavor belong to.
    fn engine(&self) -> Engine;
    /// Reads `VERSION()`; refuses the other flavor and versions the flavor doesn't support.
    fn parse_version(&self, text: &str) -> Result<ServerVersion, String>;
    fn capabilities(&self, version: ServerVersion) -> Capabilities;
    /// Databases every server has, never opened on their own.
    fn system_databases(&self) -> &'static [&'static str];
    /// The plan of one statement that was already checked (a single statement, no EXPLAIN of
    /// its own, and, to analyze, one that only reads). It runs on the editor's connection.
    async fn explain(&self, driver: &ServerDriver, statement: &str, analyze: bool) -> Result<ExplainResult, String>;
}
