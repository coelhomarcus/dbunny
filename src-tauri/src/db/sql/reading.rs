//! Tells whether a statement only reads, for the assistant and for EXPLAIN ANALYZE (which
//! runs what it explains).

use super::split::split_statements_in;
use super::Dialect;

pub const READ_REFUSED: &str = "DBunny only runs statements that read. Show the SQL to the user in a ```sql block so they can run it from the editor.";
pub const SEVERAL_REFUSED: &str = "Run one statement at a time.";
pub const TIMED_OUT: &str = "The query was stopped after 30 s.";
pub const EMPTY_REFUSED: &str = "There is no statement to run.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRefusal {
    Empty,
    Several,
    Writes,
}

impl ReadRefusal {
    pub fn message(self) -> &'static str {
        match self {
            ReadRefusal::Empty => EMPTY_REFUSED,
            ReadRefusal::Several => SEVERAL_REFUSED,
            ReadRefusal::Writes => READ_REFUSED,
        }
    }
}

/// One statement, which only reads: a query, or `SHOW`/`DESCRIBE` where the dialect has them.
pub fn check_reading(dialect: Dialect, sql: &str) -> Result<(), ReadRefusal> {
    let mut statements = split_statements_in(dialect, sql)
        .into_iter()
        .map(|statement| tokenize(dialect, &statement.text))
        .filter(|tokens| !tokens.is_empty());
    let tokens = statements.next().ok_or(ReadRefusal::Empty)?;
    if statements.next().is_some() {
        return Err(ReadRefusal::Several);
    }
    let first = tokens.iter().find_map(Tok::word);
    let shows = match dialect {
        Dialect::Postgres => first == Some("SHOW"),
        Dialect::Mysql => matches!(first, Some("SHOW" | "DESCRIBE" | "DESC")),
        Dialect::Sqlite => false,
    };
    // SHOW and DESCRIBE can't write, and `SHOW CREATE TABLE` says CREATE; only a conditional comment, which
    // MySQL runs as code, is still looked for
    let clean = if shows { !tokens.contains(&Tok::Versioned) } else { starts_as_query(&tokens) && !has_writing_word(&tokens) };
    if clean {
        Ok(())
    } else {
        Err(ReadRefusal::Writes)
    }
}

/// The first word of `sql` in upper case, after any comments.
pub fn first_word(dialect: Dialect, sql: &str) -> Option<String> {
    tokenize(dialect, sql).first().and_then(Tok::word).map(str::to_string)
}

/// Whether `sql` is a query that neither writes a file or a variable nor takes locks.
pub fn is_query(dialect: Dialect, sql: &str) -> bool {
    let tokens = tokenize(dialect, sql);
    starts_as_query(&tokens) && !writes_or_locks(&tokens)
}

#[derive(Debug, PartialEq)]
enum Tok {
    /// A name, keyword or number, in ASCII upper case
    Word(String),
    /// A string or a quoted name
    Quoted,
    /// MySQL's `/*! … */` and MariaDB's `/*M! … */`, which the server runs
    Versioned,
    Punct(char),
}

impl Tok {
    fn word(&self) -> Option<&str> {
        match self {
            Tok::Word(w) => Some(w),
            _ => None,
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$' || (c as u32) > 127
}

/// Index just after the run quoted by `quote` that starts at `i`. A doubled quote stays
/// inside; `backslash` also lets `\` escape the next character.
fn skip_quoted(chars: &[char], mut i: usize, quote: char, backslash: bool) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if backslash => i += 2,
            c if c == quote && chars.get(i + 1) == Some(&quote) => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Index just after the block comment that starts at `i`; PostgreSQL nests them.
fn skip_block_comment(chars: &[char], mut i: usize, nests: bool) -> usize {
    let mut depth = 0;
    while i < chars.len() {
        if nests && chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if !nests && depth == 0 && chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth = 1;
            i += 2;
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    chars.len()
}

/// Index just after the dollar-quoted string (`$$…$$`, `$tag$…$tag$`) that starts at `i`.
fn skip_dollar_quote(chars: &[char], i: usize) -> Option<usize> {
    if i > 0 && is_word_char(chars[i - 1]) {
        return None;
    }
    let mut j = i + 1;
    if chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    while chars.get(j).is_some_and(|&c| c.is_alphanumeric() || c == '_') {
        j += 1;
    }
    if chars.get(j) != Some(&'$') {
        return None;
    }
    let tag = &chars[i..=j];
    let mut k = j + 1;
    while k + tag.len() <= chars.len() {
        if &chars[k..k + tag.len()] == tag {
            return Some(k + tag.len());
        }
        k += 1;
    }
    Some(chars.len())
}

fn tokenize(dialect: Dialect, sql: &str) -> Vec<Tok> {
    let chars: Vec<char> = sql.chars().collect();
    let (mysql, postgres, sqlite) = (dialect == Dialect::Mysql, dialect == Dialect::Postgres, dialect == Dialect::Sqlite);
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if (c == '-' && next == Some('-') && (!mysql || chars.get(i + 2).is_none_or(|c| c.is_whitespace()))) || (mysql && c == '#') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            // MariaDB's `/*M! … */` runs on MariaDB the way `/*! … */` does on both
            if mysql && (chars.get(i + 2) == Some(&'!') || (chars.get(i + 2) == Some(&'M') && chars.get(i + 3) == Some(&'!'))) {
                tokens.push(Tok::Versioned);
            }
            i = skip_block_comment(&chars, i, postgres);
        } else if c == '\'' {
            // E'…' strings of PostgreSQL take backslash escapes
            let escapes = mysql || (postgres && i > 0 && matches!(chars[i - 1], 'e' | 'E') && (i < 2 || !is_word_char(chars[i - 2])));
            i = skip_quoted(&chars, i, '\'', escapes);
            tokens.push(Tok::Quoted);
        } else if c == '"' || c == '`' {
            i = skip_quoted(&chars, i, c, mysql && c == '"');
            tokens.push(Tok::Quoted);
        } else if sqlite && c == '[' {
            i = chars[i..].iter().position(|&c| c == ']').map_or(chars.len(), |p| i + p + 1);
            tokens.push(Tok::Quoted);
        } else if postgres && c == '$' && skip_dollar_quote(&chars, i).is_some() {
            i = skip_dollar_quote(&chars, i).unwrap_or(chars.len());
            tokens.push(Tok::Quoted);
        } else if is_word_char(c) {
            let start = i;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            tokens.push(Tok::Word(chars[start..i].iter().collect::<String>().to_ascii_uppercase()));
        } else {
            tokens.push(Tok::Punct(c));
            i += 1;
        }
    }
    tokens
}

/// Whether the tokens start as a query: SELECT, TABLE, VALUES, a parenthesis, or WITH followed
/// by one of those. After the CTEs, the verb is the first word outside parentheses that is
/// neither a CTE's name nor `AS`.
fn starts_as_query(tokens: &[Tok]) -> bool {
    let mut first = true;
    let mut with = false;
    let mut depth = 0usize;
    for token in tokens {
        match token {
            Tok::Punct('(') => {
                if first {
                    return true;
                }
                depth += 1;
            }
            Tok::Punct(')') => depth = depth.saturating_sub(1),
            Tok::Word(word) if first => match word.as_str() {
                "SELECT" | "TABLE" | "VALUES" => return true,
                "WITH" => with = true,
                _ => return false,
            },
            Tok::Word(word) if with && depth == 0 => match word.as_str() {
                "SELECT" | "TABLE" | "VALUES" => return true,
                "UPDATE" | "DELETE" | "INSERT" | "REPLACE" | "MERGE" => return false,
                _ => {}
            },
            _ if first => return false,
            _ => {}
        }
        first = false;
    }
    false
}

/// Whether a query writes a file or a variable (`INTO`) or locks rows.
fn writes_or_locks(tokens: &[Tok]) -> bool {
    tokens.iter().enumerate().any(|(at, token)| match token {
        Tok::Versioned => true,
        Tok::Word(word) => {
            let next = tokens.get(at + 1).and_then(Tok::word);
            match word.as_str() {
                "INTO" => true,
                "FOR" => matches!(next, Some("UPDATE" | "SHARE")),
                "LOCK" => next == Some("IN"),
                _ => false,
            }
        }
        _ => false,
    })
}

/// Words that write or lock wherever they stand outside strings, names and comments: a
/// data-modifying CTE hides them inside a query.
fn has_writing_word(tokens: &[Tok]) -> bool {
    writes_or_locks(tokens)
        || tokens.iter().enumerate().any(|(at, token)| match token {
            Tok::Word(word) => match word.as_str() {
                "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "TRUNCATE" | "DROP" | "ALTER" | "CREATE" | "GRANT" | "REVOKE" => true,
                "REPLACE" => tokens.get(at + 1) != Some(&Tok::Punct('(')),
                "FOR" => matches!(tokens.get(at + 1).and_then(Tok::word), Some("NO" | "KEY")),
                name => tokens.get(at + 1) == Some(&Tok::Punct('(')) && has_lasting_effect(name),
            },
            _ => false,
        })
}

/// Functions that act outside the transaction: a read-only transaction lets them through and
/// a rollback does not undo them (locks that last for the connection, other sessions, sequences,
/// settings, a remote server).
fn has_lasting_effect(name: &str) -> bool {
    name.starts_with("PG_ADVISORY_LOCK")
        || name.starts_with("PG_TRY_ADVISORY_LOCK")
        || matches!(
            name,
            "PG_TERMINATE_BACKEND"
                | "PG_CANCEL_BACKEND"
                | "PG_NOTIFY"
                | "PG_RELOAD_CONF"
                | "PG_ADVISORY_UNLOCK_ALL"
                | "DBLINK_EXEC"
                | "DBLINK"
                | "NEXTVAL"
                | "SETVAL"
                | "SET_CONFIG"
                | "LO_IMPORT"
                | "LO_EXPORT"
                | "GET_LOCK"
                | "RELEASE_LOCK"
                | "RELEASE_ALL_LOCKS"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Dialect; 3] = [Dialect::Postgres, Dialect::Sqlite, Dialect::Mysql];

    #[test]
    fn accepts_queries() {
        for dialect in ALL {
            for sql in [
                "SELECT 1",
                "select * from t limit 5;",
                "(SELECT 1) UNION (SELECT 2)",
                "WITH a AS (SELECT 1) SELECT * FROM a",
                "VALUES (1)",
                "TABLE t",
                "-- note\nSELECT 1",
            ] {
                // TABLE and VALUES are not SQLite syntax, but they are still reads
                assert_eq!(check_reading(dialect, sql), Ok(()), "{dialect:?}: {sql}");
            }
        }
    }

    #[test]
    fn accepts_show_and_describe_where_the_dialect_has_them() {
        assert_eq!(check_reading(Dialect::Postgres, "SHOW search_path"), Ok(()));
        assert_eq!(check_reading(Dialect::Sqlite, "SHOW search_path"), Err(ReadRefusal::Writes));
        for sql in ["SHOW TABLES", "DESCRIBE t", "DESC t"] {
            assert_eq!(check_reading(Dialect::Mysql, sql), Ok(()), "{sql}");
        }
        assert_eq!(check_reading(Dialect::Postgres, "DESCRIBE t"), Err(ReadRefusal::Writes));
        assert_eq!(check_reading(Dialect::Sqlite, "PRAGMA table_info(t)"), Err(ReadRefusal::Writes));
    }

    #[test]
    fn show_create_reads_though_it_says_create() {
        for sql in ["SHOW CREATE TABLE orders", "SHOW CREATE VIEW v", "SHOW GRANTS FOR CURRENT_USER"] {
            assert_eq!(check_reading(Dialect::Mysql, sql), Ok(()), "{sql}");
        }
        assert_eq!(check_reading(Dialect::Mysql, "SHOW CREATE TABLE orders; DROP TABLE orders"), Err(ReadRefusal::Several));
        assert_eq!(check_reading(Dialect::Mysql, "SHOW CREATE TABLE orders /*!50000 INTO OUTFILE 'x' */"), Err(ReadRefusal::Writes));
    }

    #[test]
    fn refuses_writes_and_locks() {
        for dialect in ALL {
            for sql in [
                "INSERT INTO t VALUES (1)",
                "UPDATE t SET a = 1",
                "DELETE FROM t",
                "WITH a AS (SELECT 1) DELETE FROM t",
                "SELECT 1 INTO @x",
                "SELECT * INTO new_t FROM t",
                "SELECT * FROM t FOR UPDATE",
                "SELECT * FROM t FOR NO KEY UPDATE",
                "SELECT * FROM t FOR KEY SHARE",
                "SELECT * FROM t LOCK IN SHARE MODE",
                "CREATE TABLE x (a int)",
                "CALL p()",
                "SET a = 1",
            ] {
                assert_eq!(check_reading(dialect, sql), Err(ReadRefusal::Writes), "{dialect:?}: {sql}");
            }
        }
    }

    #[test]
    fn refuses_writing_words_anywhere() {
        for sql in [
            "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d",
            "WITH u AS (UPDATE t SET a = 1 RETURNING *) SELECT 1",
            "WITH i AS (INSERT INTO t VALUES (1) RETURNING *) SELECT 1",
            "WITH m AS (MERGE INTO t USING s ON true WHEN MATCHED THEN DELETE) SELECT 1",
        ] {
            assert_eq!(check_reading(Dialect::Postgres, sql), Err(ReadRefusal::Writes), "{sql}");
        }
        // A conditional comment is run by MySQL, so what is inside counts
        assert_eq!(check_reading(Dialect::Mysql, "SELECT 1 /*!50000 INTO OUTFILE '/tmp/x' */"), Err(ReadRefusal::Writes));
    }

    #[test]
    fn mariadb_executable_comments_count_like_mysql_ones() {
        // MariaDB runs what is inside `/*M! … */`, so a file can be written from a "comment"
        for sql in ["SELECT a FROM t /*M! INTO OUTFILE '/tmp/x' */", "SELECT a FROM t /*M!100100 INTO @v */"] {
            assert_eq!(check_reading(Dialect::Mysql, sql), Err(ReadRefusal::Writes), "{sql}");
            assert!(!is_query(Dialect::Mysql, sql), "{sql}");
        }
        assert_eq!(check_reading(Dialect::Mysql, "SELECT 1 /* M! plain */"), Ok(()));
        assert_eq!(check_reading(Dialect::Postgres, "SELECT 1 /*M! plain */"), Ok(()));
    }

    #[test]
    fn refuses_functions_whose_effects_outlive_the_transaction() {
        for sql in [
            "SELECT pg_advisory_lock(1)",
            "SELECT pg_try_advisory_lock_shared(1)",
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity",
            "SELECT pg_catalog.pg_cancel_backend(1)",
            "SELECT pg_notify('c', 'x')",
            "SELECT dblink_exec('x', 'DROP TABLE t')",
            "SELECT nextval('s')",
            "SELECT set_config('a', 'b', false)",
            "SELECT GET_LOCK('a', 10)",
            "SELECT release_all_locks()",
        ] {
            assert_eq!(check_reading(Dialect::Postgres, sql), Err(ReadRefusal::Writes), "{sql}");
        }
        // Only a call counts; a column with the same name is a column
        assert_eq!(check_reading(Dialect::Postgres, "SELECT nextval FROM t"), Ok(()));
        assert_eq!(check_reading(Dialect::Postgres, "SELECT pg_advisory_xact_lock(1)"), Ok(()));
    }

    #[test]
    fn first_word_skips_comments() {
        assert_eq!(first_word(Dialect::Postgres, "-- note\n  /* x */ show search_path").as_deref(), Some("SHOW"));
        assert_eq!(first_word(Dialect::Mysql, "(SELECT 1)"), None);
        assert_eq!(first_word(Dialect::Sqlite, "  "), None);
    }

    #[test]
    fn ignores_words_in_strings_names_and_comments() {
        for dialect in [Dialect::Postgres, Dialect::Sqlite] {
            assert_eq!(check_reading(dialect, "SELECT \"update\" FROM t"), Ok(()));
        }
        assert_eq!(check_reading(Dialect::Mysql, "SELECT `delete` FROM t"), Ok(()));
        assert_eq!(check_reading(Dialect::Postgres, "SELECT $$insert$$"), Ok(()));
        for dialect in ALL {
            assert_eq!(check_reading(dialect, "SELECT 'delete from t' AS a"), Ok(()), "{dialect:?}");
            assert_eq!(check_reading(dialect, "SELECT 1 /* drop table t */"), Ok(()), "{dialect:?}");
            assert_eq!(check_reading(dialect, "SELECT replace(a, 'x', 'y') FROM t"), Ok(()), "{dialect:?}");
        }
    }

    #[test]
    fn one_statement_only() {
        for dialect in ALL {
            assert_eq!(check_reading(dialect, "SELECT 1; SELECT 2"), Err(ReadRefusal::Several));
            assert_eq!(check_reading(dialect, "SELECT 1;"), Ok(()));
            assert_eq!(check_reading(dialect, "   "), Err(ReadRefusal::Empty));
            assert_eq!(check_reading(dialect, "-- só comentário"), Err(ReadRefusal::Empty));
        }
    }

    #[test]
    fn is_query_keeps_the_explain_rule() {
        assert!(!is_query(Dialect::Mysql, "SHOW TABLES"));
        assert!(is_query(Dialect::Mysql, "SELECT 1"));
        assert!(!is_query(Dialect::Mysql, "SELECT * FROM t FOR UPDATE"));
        assert!(!is_query(Dialect::Mysql, ""));
    }

    #[test]
    fn refusals_have_the_agreed_messages() {
        assert_eq!(ReadRefusal::Writes.message(), READ_REFUSED);
        assert_eq!(ReadRefusal::Several.message(), "Run one statement at a time.");
        assert_eq!(ReadRefusal::Empty.message(), "There is no statement to run.");
    }
}
