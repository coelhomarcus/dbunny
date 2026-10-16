//! What differs between the SQL dialects the shared builders write.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
}

impl Dialect {
    /// `"name"`, with embedded quotes doubled.
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::Postgres => format!("\"{}\"", name.replace('"', "\"\"")),
        }
    }

    /// The placeholder of the n-th parameter (counting from 1).
    pub fn param(self, n: usize) -> String {
        match self {
            Dialect::Postgres => format!("${n}"),
        }
    }

    /// `expr` converted to text, to compare it with a pattern.
    pub fn as_text(self, expr: &str) -> String {
        match self {
            Dialect::Postgres => format!("{expr}::text"),
        }
    }

    /// Parameters one statement accepts.
    pub fn max_params(self) -> usize {
        match self {
            // The protocol counts them in 16 bits
            Dialect::Postgres => 65_535,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_dialect_matches_what_the_builders_always_wrote() {
        let d = Dialect::Postgres;
        assert_eq!(d.param(3), "$3");
        assert_eq!(d.as_text("\"name\""), "\"name\"::text");
        assert_eq!(d.quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(d.max_params(), 65_535);
    }
}
