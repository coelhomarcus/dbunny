//! What every database driver offers: a core (connection, catalog, data) every engine
//! implements, optional pieces (SQL console, transactions, schema editing, server
//! administration) and the capabilities the frontend reads to show or hide screens.

pub mod capabilities;
// The pieces aren't wired into the commands yet; these `allow`s go away as the tasks
// that move the commands behind them land.
#[allow(dead_code)]
pub mod driver;
pub mod model;
pub mod postgres;
pub mod sql;
pub mod unsupported;

use serde::{Deserialize, Serialize};

pub use capabilities::Capabilities;
#[allow(unused_imports)]
pub use capabilities::ExplainFormat;
pub use driver::Driver;
#[allow(unused_imports)]
pub use driver::{Catalog, Console, Data, SchemaEditor, ServerAdmin, Transactions};
pub use model::ConnectionInfo;
pub use unsupported::unsupported;

/// The database a driver talks to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Postgres,
}

impl Engine {
    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "PostgreSQL",
        }
    }
}
