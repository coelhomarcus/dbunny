//! What differs from one server of the MySQL family to another. The shared driver asks its
//! `Flavor` where the servers disagree and does everything else itself.

use super::version::ServerVersion;
use crate::db::{Capabilities, Engine};

pub trait Flavor: Send + Sync {
    /// The engine sessions of this flavor belong to.
    fn engine(&self) -> Engine;
    /// Reads `VERSION()`; refuses the other flavor and versions the flavor doesn't support.
    fn parse_version(&self, text: &str) -> Result<ServerVersion, String>;
    fn capabilities(&self, version: ServerVersion) -> Capabilities;
    /// Databases every server has, never opened on their own.
    fn system_databases(&self) -> &'static [&'static str];
}
