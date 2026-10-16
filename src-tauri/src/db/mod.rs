//! What every database driver offers: a core (connection, catalog, data) every engine
//! implements, optional pieces (SQL console, transactions, schema editing, server
//! administration) and the capabilities the frontend reads to show or hide screens.

pub mod capabilities;
pub mod driver;
pub mod model;
pub mod postgres;
pub mod sql;
pub mod sqlite;
pub mod unsupported;

use serde::{Deserialize, Serialize};

pub use capabilities::Capabilities;
#[allow(unused_imports)]
pub use capabilities::ExplainFormat;
pub use driver::Driver;
pub use driver::{Catalog, Console, Data, SchemaEditor, ServerAdmin, Transactions};
pub use model::ConnectionInfo;
pub use unsupported::unsupported;

/// The database a driver talks to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Postgres,
    Sqlite,
}

impl Engine {
    /// The name the engine is stored under in saved connections (the same as its serde form).
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Postgres => "postgres",
            Engine::Sqlite => "sqlite",
        }
    }

    /// The engine stored under `name`, or None when this version doesn't know it.
    pub fn from_name(name: &str) -> Option<Engine> {
        match name {
            "postgres" => Some(Engine::Postgres),
            "sqlite" => Some(Engine::Sqlite),
            _ => None,
        }
    }

    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "PostgreSQL",
            Engine::Sqlite => "SQLite",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_engine_has_its_names() {
        assert_eq!(serde_json::to_value(Engine::Sqlite).unwrap(), "sqlite");
        assert_eq!(serde_json::from_value::<Engine>("sqlite".into()).unwrap(), Engine::Sqlite);
        assert_eq!(Engine::Sqlite.as_str(), "sqlite");
        assert_eq!(Engine::from_name("sqlite"), Some(Engine::Sqlite));
        assert_eq!(Engine::Sqlite.label(), "SQLite");
    }
}
