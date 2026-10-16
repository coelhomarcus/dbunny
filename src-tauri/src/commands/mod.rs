pub mod connect;
pub mod query;
pub mod saved;
pub mod schema;
pub mod state;
pub mod storage;
pub mod pgtext;
pub mod sqlsplit;
pub mod tablefilter;
pub mod tableedit;
pub mod structure;
pub mod editor;
pub mod objects;
pub mod activity;
pub mod alter;
pub mod roles;

use crate::db::{unsupported, Engine};

/// The optional piece a command needs, or "<feature> isn't supported by <engine>".
#[allow(dead_code)] // used by the commands as they move behind the driver contract
pub(crate) fn require<'a, T: ?Sized>(piece: Option<&'a T>, engine: Engine, feature: &str) -> Result<&'a T, String> {
    piece.ok_or_else(|| unsupported(engine, feature))
}
