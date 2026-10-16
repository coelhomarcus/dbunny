//! Splits a SQLite script into its statements. SQLite itself says where a statement ends
//! (`sqlite3_complete`), so a semicolon inside a string, a comment or a trigger's
//! `BEGIN … END` body doesn't cut it.

use std::ffi::CString;

use crate::db::sql::split::Statement;

/// The statements of `sql`, in order, each without the closing semicolon. Chunks that are
/// only whitespace and comments are dropped; an unfinished last statement is kept.
pub fn split_sqlite(sql: &str) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut push = |text: &str, offset: usize| {
        if !has_content(text) {
            return;
        }
        let leading = text.chars().take_while(|c| c.is_whitespace()).count();
        let text = text.trim();
        statements.push(Statement { text: text.to_string(), offset: offset + leading });
    };

    // Where the current statement starts, in bytes and in characters
    let (mut start_byte, mut start_char) = (0, 0);
    for (char_index, (byte, c)) in sql.char_indices().enumerate() {
        if c == ';' && is_complete(&sql[start_byte..=byte]) {
            push(&sql[start_byte..byte], start_char);
            start_byte = byte + 1;
            start_char = char_index + 1;
        }
    }
    push(&sql[start_byte..], start_char);
    statements
}

/// SQLite's own answer to whether `text` ends a statement. SQLite reads up to a NUL
/// character, so only the text before one counts.
fn is_complete(text: &str) -> bool {
    let before_nul = text.split('\0').next().unwrap_or("");
    let Ok(c_text) = CString::new(before_nul) else {
        return false;
    };
    // SAFETY: a valid NUL-terminated string that lives through the call
    unsafe { rusqlite::ffi::sqlite3_complete(c_text.as_ptr()) != 0 }
}

/// Whether `text` holds anything besides whitespace and comments.
fn has_content(text: &str) -> bool {
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else {
            return !rest.is_empty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(sql: &str) -> Vec<String> {
        split_sqlite(sql).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn trigger_bodies_stay_one_statement() {
        let trigger = "CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO t SELECT CASE WHEN new.x > 1 THEN 0 ELSE 1 END; SELECT ';'; END";
        let sql = format!("CREATE TABLE t(x); {trigger}; SELECT 1;");
        assert_eq!(texts(&sql), ["CREATE TABLE t(x)", trigger, "SELECT 1"]);
        // Without the final semicolon the trigger is still the last statement, whole
        assert_eq!(texts(&format!("SELECT 1; {trigger}")), ["SELECT 1", trigger]);
    }

    #[test]
    fn semicolons_in_strings_and_comments_do_not_split() {
        assert_eq!(texts("SELECT 'a;b'; SELECT 2"), ["SELECT 'a;b'", "SELECT 2"]);
        assert_eq!(texts("SELECT 'it''s;fine'; SELECT 2"), ["SELECT 'it''s;fine'", "SELECT 2"]);
        assert_eq!(texts(r#"SELECT "a;b" FROM t; SELECT 2"#), [r#"SELECT "a;b" FROM t"#, "SELECT 2"]);
        assert_eq!(texts("SELECT [a;b] FROM t; SELECT `c;d`"), ["SELECT [a;b] FROM t", "SELECT `c;d`"]);
        assert_eq!(texts("SELECT 1 -- a;b\n; SELECT 2"), ["SELECT 1 -- a;b", "SELECT 2"]);
        assert_eq!(texts("SELECT 1 /* a;b */; SELECT 2"), ["SELECT 1 /* a;b */", "SELECT 2"]);
        // A backslash is just a character in SQLite strings
        assert_eq!(texts(r"SELECT 'a\'; SELECT 2"), [r"SELECT 'a\'", "SELECT 2"]);
        // Unfinished quotes and comments swallow the rest
        assert_eq!(texts("SELECT 'oops; SELECT 2"), ["SELECT 'oops; SELECT 2"]);
        assert_eq!(texts("SELECT 1 /* oops; SELECT 2"), ["SELECT 1 /* oops; SELECT 2"]);
    }

    #[test]
    fn comment_only_chunks_and_stray_semicolons_are_dropped() {
        assert!(texts("-- nothing here").is_empty());
        assert!(texts("/* nothing */").is_empty());
        assert!(texts(";;; ;").is_empty());
        assert_eq!(texts("SELECT 1; -- trailing note"), ["SELECT 1"]);
        assert_eq!(texts("SELECT 1; /* a */ ; /* b */"), ["SELECT 1"]);
        // A comment before a statement stays with it, like the PostgreSQL splitter
        assert_eq!(texts("-- first\nSELECT 1"), ["-- first\nSELECT 1"]);
    }

    #[test]
    fn empty_and_whitespace_only_input_gives_nothing() {
        assert!(texts("").is_empty());
        assert!(texts("   \n\t ").is_empty());
    }

    #[test]
    fn offsets_count_characters_from_the_start_of_the_script() {
        let statements = split_sqlite("SELECT 1;\n  SELECT bad");
        assert_eq!(statements[0].offset, 0);
        assert_eq!(statements[1].offset, 12);
        assert_eq!(statements[1].text, "SELECT bad");
        let statements = split_sqlite("SELECT 'ação'; SELECT 2");
        assert_eq!(statements[1].offset, 15);
        assert_eq!(statements[1].text, "SELECT 2");
    }

    #[test]
    fn a_nul_character_ends_what_sqlite_reads() {
        // SQLite stops reading at the NUL, so the semicolon after it can't end a statement
        assert_eq!(texts("SELECT 1; SELECT '\0'; SELECT 2"), ["SELECT 1", "SELECT '\0'; SELECT 2"]);
    }
}
