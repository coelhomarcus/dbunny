pub mod catalog;
pub mod connect;
pub mod console;
pub mod data;
pub mod saved;
pub mod state;
pub mod storage;
pub mod transactions;
pub mod objects;
pub mod activity;
pub mod alter;
pub mod roles;

use crate::db::{unsupported, Engine};

/// The optional piece a command needs, or "<feature> isn't supported by <engine>".
pub(crate) fn require<'a, T: ?Sized>(piece: Option<&'a T>, engine: Engine, feature: &str) -> Result<&'a T, String> {
    piece.ok_or_else(|| unsupported(engine, feature))
}
