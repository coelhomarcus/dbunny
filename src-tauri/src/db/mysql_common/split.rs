//! Splits a MySQL script into its statements.
//!
//! The editor runs one statement at a time, so a script is cut at the semicolons that end a
//! statement. A semicolon doesn't end one inside a string, a backtick name or a comment, nor
//! inside the `BEGIN … END` body of a `CREATE PROCEDURE/FUNCTION/TRIGGER/EVENT`: there the
//! statement only ends once the blocks the body opened (BEGIN, CASE, IF, LOOP, WHILE, REPEAT)
//! have closed. The editor's splitter (`splitMysql` in `web/src/lib/sqlStatements.ts`) follows
//! the same rules; both run the cases in `split_cases.json`.

use crate::db::sql::split::Statement;

pub const DELIMITER_IS_A_CLIENT_COMMAND: &str =
    "DELIMITER is a command of the mysql client, not SQL; DBunny already knows where a procedure body ends.";

/// The statements of `sql`, each without the closing semicolon. A `DELIMITER` line is an
/// error: it's a command of the mysql client, and this splitter already knows where a body ends.
#[allow(dead_code)] // used by the console (Task 9)
pub fn split_mysql(sql: &str) -> Result<Vec<Statement>, String> {
    let mut statements = Vec::new();
    for piece in split(sql) {
        if piece.delimiter {
            return Err(DELIMITER_IS_A_CLIENT_COMMAND.to_string());
        }
        statements.push(piece.statement);
    }
    Ok(statements)
}

/// `split_mysql` without the error: a `DELIMITER` line becomes a statement of its own, the way
/// the editor shows it.
pub fn split_mysql_lenient(sql: &str) -> Vec<Statement> {
    split(sql).into_iter().map(|piece| piece.statement).collect()
}

struct Piece {
    statement: Statement,
    /// The statement is a `DELIMITER …` line
    delimiter: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    /// A name, keyword or number, in ASCII upper case
    Word(String),
    /// A string or a backtick name
    Quoted,
    /// `/*! … */`: MySQL runs what's inside, so it counts as part of a statement
    Versioned,
    Semi,
    Punct(char),
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Token {
    fn word(&self) -> Option<&str> {
        match &self.kind {
            Kind::Word(w) => Some(w),
            _ => None,
        }
    }

    fn is_punct(&self, c: char) -> bool {
        self.kind == Kind::Punct(c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Block {
    Begin,
    If,
    Loop,
    While,
    Repeat,
    /// `CASE` as a statement of the body: its `THEN` and `ELSE` hold statements
    CaseStatement,
    /// `CASE` inside an expression: closed by a plain `END`
    CaseExpression,
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$' || (c as u32) > 127
}

/// Index just after a quoted run (`'…'`, `"…"`, `` `…` ``) starting at `i`. Strings take a
/// backslash escape and a doubled quote; backtick names only the doubled backtick.
fn skip_quoted(chars: &[char], mut i: usize) -> usize {
    let quote = chars[i];
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if quote != '`' => i += 2,
            c if c == quote && chars.get(i + 1) == Some(&quote) => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Index of the next `\n` at or after `i`, or the end of the script.
fn line_end(chars: &[char], i: usize) -> usize {
    chars[i.min(chars.len())..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| i + p)
}

/// Index just after the `*/` that closes a block comment opened at `i` (they don't nest).
fn block_comment_end(chars: &[char], i: usize) -> usize {
    let mut j = i + 2;
    while j + 1 < chars.len() {
        if chars[j] == '*' && chars[j + 1] == '/' {
            return j + 2;
        }
        j += 1;
    }
    chars.len()
}

/// The next token at or after `i`, past whitespace and comments.
pub(crate) fn next_token(chars: &[char], mut i: usize) -> Option<Token> {
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' {
            i = line_end(chars, i);
        } else if c == '-' && next == Some('-') && chars.get(i + 2).is_none_or(|c| c.is_whitespace()) {
            // `--` is a comment only before whitespace: `1--1` is one minus minus one
            i = line_end(chars, i);
        } else if c == '/' && next == Some('*') {
            let end = block_comment_end(chars, i);
            if chars.get(i + 2) == Some(&'!') {
                return Some(Token { kind: Kind::Versioned, start: i, end });
            }
            i = end;
        } else if c == '\'' || c == '"' || c == '`' {
            return Some(Token { kind: Kind::Quoted, start: i, end: skip_quoted(chars, i) });
        } else if c == ';' {
            return Some(Token { kind: Kind::Semi, start: i, end: i + 1 });
        } else if is_word_char(c) && !(c == '$' && next == Some('$')) {
            let mut end = i + 1;
            // `$$` ends a word: `END$$` is END and the delimiter of a mysql client script
            while end < chars.len() && is_word_char(chars[end]) && !(chars[end] == '$' && chars.get(end + 1) == Some(&'$')) {
                end += 1;
            }
            let word: String = chars[i..end].iter().collect();
            return Some(Token { kind: Kind::Word(word.to_ascii_uppercase()), start: i, end });
        } else {
            return Some(Token { kind: Kind::Punct(c), start: i, end: i + 1 });
        }
    }
    None
}

/// Whether the first tokens of a statement are `CREATE [OR REPLACE] [DEFINER = user]
/// [AGGREGATE] PROCEDURE|FUNCTION|TRIGGER|EVENT`; `None` while there are too few to tell.
fn routine_header(tokens: &[Kind]) -> Option<bool> {
    let word = |i: usize| match tokens.get(i) {
        Some(Kind::Word(w)) => Some(w.as_str()),
        _ => None,
    };
    let name = |i: usize| matches!(tokens.get(i), Some(Kind::Word(_) | Kind::Quoted));
    let punct = |i: usize, c: char| tokens.get(i) == Some(&Kind::Punct(c));
    macro_rules! need {
        ($i:expr) => {
            if $i >= tokens.len() {
                return None;
            }
        };
    }

    need!(0);
    if word(0) != Some("CREATE") {
        return Some(false);
    }
    let mut i = 1;
    need!(i);
    if word(i) == Some("OR") {
        need!(i + 1);
        if word(i + 1) != Some("REPLACE") {
            return Some(false);
        }
        i += 2;
    }
    need!(i);
    if word(i) == Some("DEFINER") {
        need!(i + 1);
        if !punct(i + 1, '=') {
            return Some(false);
        }
        i += 2;
        need!(i);
        if word(i) == Some("CURRENT_USER") {
            i += 1;
            need!(i);
            if punct(i, '(') {
                need!(i + 1);
                if !punct(i + 1, ')') {
                    return Some(false);
                }
                i += 2;
            }
        } else {
            // user, or user@host, each a name or a quoted string
            if !name(i) {
                return Some(false);
            }
            i += 1;
            need!(i);
            if punct(i, '@') {
                need!(i + 1);
                if !name(i + 1) {
                    return Some(false);
                }
                i += 2;
            }
        }
    }
    need!(i);
    if word(i) == Some("AGGREGATE") {
        i += 1;
    }
    need!(i);
    Some(matches!(word(i), Some("PROCEDURE" | "FUNCTION" | "TRIGGER" | "EVENT")))
}

/// The keyword that may follow END to close `block`, as in `END IF`.
fn is_end_keyword(word: Option<&str>) -> bool {
    matches!(word, Some("IF" | "CASE" | "LOOP" | "WHILE" | "REPEAT"))
}

/// Before an END that closes a CASE expression comes a value; after these an operand is
/// still due, so an `end` there is a column name.
fn expects_operand(token: Option<&Token>) -> bool {
    match token.map(|t| &t.kind) {
        Some(Kind::Punct(c)) => ",(=<>+-*/%!&|^~".contains(*c),
        Some(Kind::Word(w)) => matches!(w.as_str(), "WHEN" | "THEN" | "ELSE" | "CASE" | "AND" | "OR" | "NOT"),
        _ => false,
    }
}

/// What the splitter knows about the statement it's in.
#[derive(Default)]
struct Current {
    /// Where its text starts (just after the previous statement)
    pub(crate) start: usize,
    /// It has something besides whitespace and comments
    has_content: bool,
    /// Its first tokens, until `routine` is decided
    header: Vec<Kind>,
    /// It's a CREATE PROCEDURE/FUNCTION/TRIGGER/EVENT (`None` while undecided)
    routine: Option<bool>,
    /// The blocks of the body that are open, innermost last
    blocks: Vec<Block>,
    /// The next word starts a statement of the body (after `;`, BEGIN, THEN, a label…)
    body_start: bool,
    /// The statement of the body is a DECLARE, whose HANDLER may be a BEGIN … END block
    in_declare: bool,
    /// The body opened a block and all its blocks have closed again
    body_closed: bool,
    previous: Option<Token>,
}

impl Current {
    fn at(start: usize) -> Self {
        Current { start, ..Default::default() }
    }

    fn open(&mut self, block: Block) {
        self.blocks.push(block);
        self.in_declare = false;
    }

    fn close(&mut self) {
        self.blocks.pop();
        if self.blocks.is_empty() {
            self.body_closed = true;
        }
    }

    fn in_body(&self) -> bool {
        self.routine == Some(true)
    }
}

fn split(sql: &str) -> Vec<Piece> {
    let chars: Vec<char> = sql.chars().collect();
    let mut pieces = Vec::new();
    let mut push = |from: usize, to: usize, delimiter: bool| {
        let slice = &chars[from..to];
        let leading = slice.iter().take_while(|c| c.is_whitespace()).count();
        let text: String = slice[leading..].iter().collect();
        let text = text.trim_end().to_string();
        pieces.push(Piece { statement: Statement { text, offset: from + leading }, delimiter });
    };

    let mut current = Current::at(0);
    let mut i = 0;
    while let Some(token) = next_token(&chars, i) {
        i = token.end;

        if token.kind == Kind::Semi {
            if current.in_body() && !current.blocks.is_empty() {
                // A statement of the body ends, the CREATE goes on
                current.body_start = true;
                current.in_declare = false;
                current.previous = Some(token);
                continue;
            }
            if current.has_content {
                push(current.start, token.start, false);
            }
            current = Current::at(token.end);
            continue;
        }

        if token.word() == Some("DELIMITER") && chars.get(token.end).is_none_or(|c| c.is_whitespace()) {
            let line_start = chars[..token.start].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
            let first_on_line = chars[line_start..token.start].iter().all(|c| c.is_whitespace());
            // At the start of a statement, or on the line after a body that closed (`END$$`)
            let after_body = current.in_body() && current.body_closed && current.blocks.is_empty() && first_on_line;
            if !current.has_content || after_body {
                let from = if current.has_content {
                    push(current.start, line_start, false);
                    line_start
                } else {
                    current.start
                };
                let end = line_end(&chars, token.end);
                push(from, end, true);
                current = Current::at(end);
                i = end;
                continue;
            }
        }

        current.has_content = true;
        if current.routine.is_none() {
            current.header.push(token.kind.clone());
            current.routine = routine_header(&current.header);
            current.previous = Some(token);
            continue;
        }
        if !current.in_body() {
            current.previous = Some(token);
            continue;
        }

        let body_start = std::mem::take(&mut current.body_start);
        let Some(word) = token.word() else {
            current.previous = Some(token);
            continue;
        };
        let peek = next_token(&chars, i);
        let next_word = peek.as_ref().and_then(|t| t.word());
        // `t.end`, `@end`, `x$end`: a name, never a keyword
        let named = current.previous.as_ref().is_some_and(|p| p.is_punct('.') || p.is_punct('@'));

        // `label:` (not `:=`) is followed by a statement of the body
        if let Some(colon) = peek.as_ref().filter(|t| t.is_punct(':')) {
            if chars.get(colon.end) != Some(&'=') {
                i = colon.end;
                current.body_start = true;
                current.previous = peek;
                continue;
            }
        }

        match word {
            _ if named => {}
            "BEGIN" => {
                let in_a_list = current.previous.as_ref().is_some_and(|p| p.is_punct(',') || p.is_punct('('));
                let transaction = next_word == Some("WORK") || peek.as_ref().is_some_and(|t| t.kind == Kind::Semi);
                if !in_a_list && !transaction && (body_start || current.blocks.is_empty() || current.in_declare) {
                    current.open(Block::Begin);
                    current.body_start = true;
                }
            }
            "CASE" => current.open(if body_start { Block::CaseStatement } else { Block::CaseExpression }),
            "IF" if body_start => current.open(Block::If),
            "WHILE" if body_start => current.open(Block::While),
            "LOOP" if body_start => {
                current.open(Block::Loop);
                current.body_start = true;
            }
            "REPEAT" if body_start => {
                current.open(Block::Repeat);
                current.body_start = true;
            }
            "END" => {
                let closes = match current.blocks.last() {
                    None => false,
                    Some(Block::CaseExpression) => !expects_operand(current.previous.as_ref()),
                    // END after a statement of the body, or `END IF` (also `UNTIL … END REPEAT`)
                    Some(_) => body_start || is_end_keyword(next_word),
                };
                if closes {
                    current.close();
                    if is_end_keyword(next_word) {
                        i = peek.as_ref().map_or(i, |t| t.end);
                        current.previous = peek;
                        continue;
                    }
                }
            }
            "THEN" | "ELSE" => {
                if matches!(current.blocks.last(), Some(Block::If | Block::CaseStatement)) {
                    current.body_start = true;
                }
            }
            // `WHILE … DO` and an event's `DO`; a DO that starts a statement is the DO statement
            "DO" if !body_start => current.body_start = true,
            "DECLARE" if body_start => current.in_declare = true,
            // A trigger's body comes after FOR EACH ROW [FOLLOWS|PRECEDES other]
            "ROW" if current.blocks.is_empty() && current.previous.as_ref().and_then(|p| p.word()) == Some("EACH") => {
                current.body_start = true;
            }
            "FOLLOWS" | "PRECEDES" if body_start && current.blocks.is_empty() => {
                if let Some(other) = peek {
                    i = other.end;
                    current.body_start = true;
                    current.previous = Some(other);
                    continue;
                }
            }
            _ => {}
        }
        current.previous = Some(token);
    }
    if current.has_content {
        push(current.start, chars.len(), false);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        sql: String,
        statements: Vec<String>,
        error: Option<String>,
    }

    fn cases() -> Vec<Case> {
        serde_json::from_str(include_str!("split_cases.json")).expect("split_cases.json is valid")
    }

    fn check_offsets(case: &Case, statements: &[Statement]) {
        for statement in statements {
            let from_offset: String = case.sql.chars().skip(statement.offset).take(statement.text.chars().count()).collect();
            assert_eq!(from_offset, statement.text, "{}: offset {} doesn't point at the statement", case.name, statement.offset);
        }
    }

    #[test]
    fn split_cases_json_holds() {
        let cases = cases();
        assert!(cases.len() >= 11);
        for case in &cases {
            let lenient = split_mysql_lenient(&case.sql);
            let texts: Vec<&str> = lenient.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(texts, case.statements, "{}", case.name);
            check_offsets(case, &lenient);

            match (&case.error, split_mysql(&case.sql)) {
                (Some(expected), Err(message)) => assert_eq!(&message, expected, "{}", case.name),
                (Some(_), Ok(_)) => panic!("{}: expected an error", case.name),
                (None, Err(message)) => panic!("{}: unexpected error {message}", case.name),
                (None, Ok(statements)) => {
                    assert_eq!(statements, lenient, "{}", case.name);
                }
            }
        }
    }

    #[test]
    fn the_required_cases_are_there() {
        let names: Vec<String> = cases().into_iter().map(|c| c.name).collect();
        for required in [
            "two_selects",
            "semicolon_in_strings_and_backticks",
            "hash_and_dash_comments",
            "block_and_versioned_comments",
            "procedure_with_nested_blocks",
            "case_expression_inside_a_body",
            "trigger_with_definer",
            "begin_at_top_level_is_a_transaction",
            "if_function_is_not_a_block",
            "labels_and_loops",
            "delimiter_line",
        ] {
            assert!(names.iter().any(|n| n == required), "missing case {required}");
        }
    }
}
