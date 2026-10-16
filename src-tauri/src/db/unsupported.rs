//! The answer when a command asks a driver for a piece its engine doesn't have.

use super::Engine;

/// "Roles isn't supported by SQLite". The screens hide what an engine lacks; this is
/// the fallback if a call gets through anyway.
pub fn unsupported(engine: Engine, feature: &str) -> String {
    format!("{feature} isn't supported by {}", engine.label())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_names_the_feature_and_the_engine() {
        assert_eq!(unsupported(Engine::Postgres, "Roles"), "Roles isn't supported by PostgreSQL");
    }
}
