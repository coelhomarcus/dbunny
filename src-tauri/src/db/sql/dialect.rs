//! What differs between the SQL dialects the shared builders write.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    Sqlite,
    Mysql,
}

impl Dialect {
    /// `"name"`, with embedded quotes doubled.
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::Postgres | Dialect::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
            Dialect::Mysql => format!("`{}`", name.replace('`', "``")),
        }
    }

    /// The placeholder of the n-th parameter (counting from 1).
    pub fn param(self, n: usize) -> String {
        match self {
            Dialect::Postgres => format!("${n}"),
            Dialect::Sqlite => format!("?{n}"),
            // Positional: the n-th placeholder is the n-th `?`
            Dialect::Mysql => "?".to_string(),
        }
    }

    /// `expr` converted to text, to compare it with a pattern.
    pub fn as_text(self, expr: &str) -> String {
        match self {
            Dialect::Postgres => format!("{expr}::text"),
            Dialect::Sqlite => format!("CAST({expr} AS TEXT)"),
            Dialect::Mysql => format!("CAST({expr} AS CHAR)"),
        }
    }

    /// Parameters one statement accepts.
    pub fn max_params(self) -> usize {
        match self {
            // The protocol counts them in 16 bits
            Dialect::Postgres | Dialect::Mysql => 65_535,
            // SQLITE_MAX_VARIABLE_NUMBER since 3.32
            Dialect::Sqlite => 32_766,
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

    #[test]
    fn mysql_dialect_quotes_with_backticks_and_binds_with_question_marks() {
        let d = Dialect::Mysql;
        assert_eq!(d.quote_ident("a`b"), "`a``b`");
        assert_eq!(d.param(3), "?");
        assert_eq!(d.as_text("`name`"), "CAST(`name` AS CHAR)");
        assert_eq!(d.max_params(), 65_535);
    }
}
