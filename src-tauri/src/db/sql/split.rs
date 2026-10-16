//! Splits a SQL script into its statements.
//!
//! The driver only runs one statement at a time, so a script like `SELECT 1; SELECT 2;`
//! has to be cut at the semicolons that actually end a statement. A semicolon doesn't
//! end one inside a string, a quoted identifier, a comment or a dollar-quoted body
//! (function definitions, `DO` blocks).

use super::Dialect;

#[derive(Debug, PartialEq)]
pub struct Statement {
    /// The statement text, without surrounding whitespace or the closing semicolon.
    pub text: String,
    /// Where it starts in the script, in characters (error positions are relative to it).
    pub offset: usize,
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || (c as u32) > 127
}

/// Index just after a `'...'` string starting at `i` (which holds the opening quote).
fn skip_string(chars: &[char], mut i: usize, backslash_escapes: bool) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if backslash_escapes => i += 2,
            '\'' if chars.get(i + 1) == Some(&'\'') => i += 2,
            '\'' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Index just after a `"..."` identifier starting at `i`.
fn skip_identifier(chars: &[char], mut i: usize) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '"' if chars.get(i + 1) == Some(&'"') => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Index just after a (possibly nested) block comment starting at `i`.
fn skip_block_comment(chars: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth += 1;
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

/// If a dollar-quoted string (`$$...$$` or `$tag$...$tag$`) starts at `i`, the index after it.
fn skip_dollar_quote(chars: &[char], i: usize) -> Option<usize> {
    // `a$b` is an identifier with a dollar sign, not a quote
    if i > 0 && is_identifier_char(chars[i - 1]) {
        return None;
    }
    let mut j = i + 1;
    // The tag can't start with a digit: `$1` is a parameter
    if chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    while chars.get(j).is_some_and(|&c| is_identifier_char(c)) {
        j += 1;
    }
    if chars.get(j) != Some(&'$') {
        return None;
    }
    let tag: Vec<char> = chars[i..=j].to_vec();
    let mut k = j + 1;
    while k + tag.len() <= chars.len() {
        if chars[k..k + tag.len()] == tag[..] {
            return Some(k + tag.len());
        }
        k += 1;
    }
    // Unterminated: the quote runs to the end of the script
    Some(chars.len())
}

pub fn split_statements(sql: &str) -> Vec<Statement> {
    split_statements_in(Dialect::Postgres, sql)
}

/// `split_statements` with the lexical rules of the given dialect.
pub fn split_statements_in(dialect: Dialect, sql: &str) -> Vec<Statement> {
    match dialect {
        Dialect::Postgres => split_postgres(sql),
    }
}

fn split_postgres(sql: &str) -> Vec<Statement> {
    let chars: Vec<char> = sql.chars().collect();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut has_content = false;
    let mut i = 0;

    let mut push = |from: usize, to: usize| {
        let slice = &chars[from..to];
        let leading = slice.iter().take_while(|c| c.is_whitespace()).count();
        let text: String = slice[leading..].iter().collect();
        let text = text.trim_end().to_string();
        statements.push(Statement { text, offset: from + leading });
    };

    while i < chars.len() {
        match chars[i] {
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => i = skip_block_comment(&chars, i),
            '\'' => {
                has_content = true;
                // E'...' strings treat backslash as an escape
                let escapes = i > 0
                    && matches!(chars[i - 1], 'e' | 'E')
                    && !(i > 1 && is_identifier_char(chars[i - 2]));
                i = skip_string(&chars, i, escapes);
            }
            '"' => {
                has_content = true;
                i = skip_identifier(&chars, i);
            }
            '$' => {
                has_content = true;
                i = skip_dollar_quote(&chars, i).unwrap_or(i + 1);
            }
            ';' => {
                if has_content {
                    push(start, i);
                }
                start = i + 1;
                has_content = false;
                i += 1;
            }
            c => {
                if !c.is_whitespace() {
                    has_content = true;
                }
                i += 1;
            }
        }
    }
    if has_content {
        push(start, chars.len());
    }
    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(sql: &str) -> Vec<String> {
        split_statements(sql).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn splits_on_semicolons() {
        assert_eq!(texts("SELECT 1; SELECT 2;"), ["SELECT 1", "SELECT 2"]);
        assert_eq!(texts("SELECT 1;\n\nSELECT 2"), ["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn a_single_statement_needs_no_semicolon() {
        assert_eq!(texts("SELECT 1"), ["SELECT 1"]);
        assert_eq!(texts("  SELECT 1  ;  "), ["SELECT 1"]);
    }

    #[test]
    fn empty_input_and_stray_semicolons_give_nothing() {
        assert!(texts("").is_empty());
        assert!(texts("   \n ").is_empty());
        assert!(texts(";;; ;").is_empty());
    }

    #[test]
    fn comment_only_chunks_are_dropped() {
        assert!(texts("-- nothing here").is_empty());
        assert!(texts("/* nothing */").is_empty());
        assert_eq!(texts("SELECT 1; -- trailing note"), ["SELECT 1"]);
        assert_eq!(texts("SELECT 1; /* a */ ; /* b */"), ["SELECT 1"]);
    }

    #[test]
    fn semicolons_inside_strings_do_not_split() {
        assert_eq!(texts("SELECT 'a;b'; SELECT 2"), ["SELECT 'a;b'", "SELECT 2"]);
        assert_eq!(texts("SELECT 'it''s;fine'; SELECT 2"), ["SELECT 'it''s;fine'", "SELECT 2"]);
    }

    #[test]
    fn escape_strings_honour_backslash_quotes() {
        assert_eq!(texts(r"SELECT E'a\';b'; SELECT 2"), [r"SELECT E'a\';b'", "SELECT 2"]);
        // In a normal string a backslash is just a character
        assert_eq!(texts(r"SELECT 'a\'; SELECT 2"), [r"SELECT 'a\'", "SELECT 2"]);
    }

    #[test]
    fn e_inside_an_identifier_is_not_an_escape_prefix() {
        assert_eq!(texts(r"SELECT name'x\'; SELECT 2"), [r"SELECT name'x\'", "SELECT 2"]);
    }

    #[test]
    fn semicolons_inside_quoted_identifiers_do_not_split() {
        assert_eq!(texts(r#"SELECT "a;b" FROM t; SELECT 2"#), [r#"SELECT "a;b" FROM t"#, "SELECT 2"]);
        assert_eq!(texts(r#"SELECT "a""b;c"; SELECT 2"#), [r#"SELECT "a""b;c""#, "SELECT 2"]);
    }

    #[test]
    fn semicolons_inside_comments_do_not_split() {
        assert_eq!(texts("SELECT 1 -- a;b\n; SELECT 2"), ["SELECT 1 -- a;b", "SELECT 2"]);
        assert_eq!(texts("SELECT 1 /* a;b */; SELECT 2"), ["SELECT 1 /* a;b */", "SELECT 2"]);
        assert_eq!(texts("SELECT 1 /* a /* ; */ ; */; SELECT 2"), ["SELECT 1 /* a /* ; */ ; */", "SELECT 2"]);
    }

    #[test]
    fn dollar_quoted_bodies_are_kept_whole() {
        let function = "CREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END; $$ LANGUAGE plpgsql";
        assert_eq!(texts(&format!("{function}; SELECT f()")), [function, "SELECT f()"]);

        let tagged = "DO $body$ BEGIN PERFORM 1; PERFORM 2; END $body$";
        assert_eq!(texts(&format!("{tagged}; SELECT 1")), [tagged, "SELECT 1"]);
    }

    #[test]
    fn a_different_dollar_tag_inside_a_body_does_not_close_it() {
        let sql = "SELECT $a$ x $b$ ; $b$ y $a$; SELECT 2";
        assert_eq!(texts(sql), ["SELECT $a$ x $b$ ; $b$ y $a$", "SELECT 2"]);
    }

    #[test]
    fn parameters_and_dollar_identifiers_are_not_dollar_quotes() {
        assert_eq!(texts("SELECT $1; SELECT $2"), ["SELECT $1", "SELECT $2"]);
        assert_eq!(texts("SELECT a$b$c; SELECT 2"), ["SELECT a$b$c", "SELECT 2"]);
    }

    #[test]
    fn unterminated_quotes_swallow_the_rest_instead_of_looping() {
        assert_eq!(texts("SELECT 'oops; SELECT 2"), ["SELECT 'oops; SELECT 2"]);
        assert_eq!(texts("SELECT $$oops; SELECT 2"), ["SELECT $$oops; SELECT 2"]);
        assert_eq!(texts("SELECT 1 /* oops; SELECT 2"), ["SELECT 1 /* oops; SELECT 2"]);
    }

    #[test]
    fn offsets_count_characters_so_error_positions_can_be_shifted() {
        let statements = split_statements("SELECT 1;\n  SELECT bad");
        assert_eq!(statements[0].offset, 0);
        // Leading whitespace is not part of the statement
        assert_eq!(statements[1].offset, 12);
        assert_eq!(statements[1].text, "SELECT bad");
    }

    #[test]
    fn offsets_are_in_characters_not_bytes() {
        let statements = split_statements("SELECT 'ação'; SELECT 2");
        assert_eq!(statements[1].offset, 15);
    }
}
