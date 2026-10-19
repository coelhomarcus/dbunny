//! What differs from one server of the MySQL family to another. The shared driver asks its
//! `Flavor` where the servers disagree and does everything else itself.

use async_trait::async_trait;

use super::admin::RawSession;
use super::driver::ServerDriver;
use super::version::ServerVersion;
use crate::db::model::{ExplainResult, RoleInfo};
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

    /// The accounts of the server, roles included.
    async fn accounts(&self, conn: &mut mysql_async::Conn) -> mysql_async::Result<Vec<RoleInfo>>;
    /// Only the account of this session, for one that can't read the accounts.
    async fn own_account(&self, conn: &mut mysql_async::Conn) -> mysql_async::Result<RoleInfo>;
    /// The sessions with their transactions and applications (the full picture), limited to
    /// those using `database` when it is given. Fails when the account can't see so much.
    async fn rich_sessions(&self, conn: &mut mysql_async::Conn, database: Option<&str>) -> mysql_async::Result<Vec<RawSession>>;
    /// Who waits on whom, as `(waiting, blocking)` process ids; empty when it can't be seen.
    async fn blockers(&self, conn: &mut mysql_async::Conn) -> Vec<(i64, i64)>;
    /// The ids of the other sessions that use database `name`.
    async fn session_ids_in(&self, conn: &mut mysql_async::Conn, name: &str) -> mysql_async::Result<Vec<i64>>;
}
