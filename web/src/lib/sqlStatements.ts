/** Where one statement of a script starts and ends (without the closing semicolon). */
export interface StatementRange {
  from: number;
  to: number;
}

/** The SQL a connection speaks, which decides where its statements end. */
export type SqlDialectName = "postgres" | "sqlite" | "mysql";

/**
 * Splits a script at the semicolons that end statements, skipping the ones inside
 * strings, quoted names and comments. PostgreSQL also has E'...' strings, nested block
 * comments and dollar-quoted bodies ($$...$$, $tag$...$tag$); SQLite has `...` and [...]
 * names and triggers whose body holds statements; MySQL has # comments, backslash escapes
 * and procedure bodies. Empty statements are dropped.
 */
export function splitStatements(sql: string, dialect: SqlDialectName = "postgres"): StatementRange[] {
  if (dialect === "sqlite") return splitSqlite(sql);
  if (dialect === "mysql") return splitMysql(sql);
  return splitPostgres(sql);
}

/** Adds the statement between `from` and `to` (trimmed) unless it's empty or only comments. */
function pushStatement(ranges: StatementRange[], sql: string, from: number, to: number) {
  while (from < to && /\s/.test(sql[from])) from++;
  while (to > from && /\s/.test(sql[to - 1])) to--;
  // A statement that is only comments still runs as nothing; keep it out
  if (to > from && sql.slice(from, to).replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, "").trim()) ranges.push({ from, to });
}

/** True for `[EXPLAIN] CREATE [TEMP|TEMPORARY] TRIGGER`, given the first words of a statement. */
function startsTrigger(words: string[]) {
  const [a, b, c] = words[0] === "EXPLAIN" ? words.slice(1) : words;
  return a === "CREATE" && (b === "TRIGGER" || ((b === "TEMP" || b === "TEMPORARY") && c === "TRIGGER"));
}

const SQLITE_WORD_START = /[A-Za-z_\u0080-\uffff]/;
const SQLITE_WORD_PART = /[A-Za-z0-9_$\u0080-\uffff]/;

/**
 * SQLite's statements end at a semicolon, except inside a trigger, whose body holds statements of
 * its own. There SQLite itself (sqlite3_complete) closes the trigger only at an END that comes right
 * after a semicolon and is itself followed by one: begin, case and end can also be column names.
 */
function splitSqlite(sql: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  let start = 0;
  let i = 0;
  // The first words of the current statement, to know when it's a trigger
  let words: string[] = [];
  // Inside a trigger: the last token was a semicolon / an END that came right after one
  let afterSemi = false;
  let afterEnd = false;
  const other = () => {
    afterSemi = false;
    afterEnd = false;
  };
  const trigger = () => words.length >= 3 && startsTrigger(words);

  while (i < sql.length) {
    const ch = sql[i];
    const next = sql[i + 1];
    if (ch === "-" && next === "-") {
      const end = sql.indexOf("\n", i);
      i = end === -1 ? sql.length : end + 1;
    } else if (ch === "/" && next === "*") {
      // SQLite's block comments don't nest
      const end = sql.indexOf("*/", i + 2);
      i = end === -1 ? sql.length : end + 2;
    } else if (ch === "'" || ch === '"' || ch === "`") {
      i++;
      while (i < sql.length) {
        if (sql[i] === ch && sql[i + 1] === ch) i += 2;
        else if (sql[i] === ch) {
          i++;
          break;
        } else i++;
      }
      if (words.length < 4) words.push(ch);
      other();
    } else if (ch === "[") {
      const end = sql.indexOf("]", i + 1);
      i = end === -1 ? sql.length : end + 1;
      if (words.length < 4) words.push("[");
      other();
    } else if (SQLITE_WORD_START.test(ch)) {
      let end = i + 1;
      while (end < sql.length && SQLITE_WORD_PART.test(sql[end])) end++;
      const word = sql.slice(i, end).toUpperCase();
      if (words.length < 4) words.push(word);
      if (trigger()) {
        if (word === "END" && afterSemi) {
          afterSemi = false;
          afterEnd = true;
        } else other();
      }
      i = end;
    } else if (ch === ";") {
      if (trigger() && !afterEnd) {
        // A statement of the trigger's body ends; only an END after it can close the trigger
        afterSemi = true;
        afterEnd = false;
        i++;
      } else {
        pushStatement(ranges, sql, start, i);
        start = i + 1;
        words = [];
        other();
        i++;
      }
    } else {
      if (!/\s/.test(ch)) other();
      i++;
    }
  }
  pushStatement(ranges, sql, start, sql.length);
  return ranges;
}

interface MysqlToken {
  kind: "word" | "quoted" | "versioned" | "semi" | "punct";
  /** A word in ASCII upper case, or the punctuation character. */
  text: string;
  start: number;
  end: number;
}

type MysqlBlock = "begin" | "if" | "loop" | "while" | "repeat" | "caseStatement" | "caseExpression";

const MYSQL_WORD = /[A-Za-z0-9_$\u0080-￿]/;

/** A word character, except the `$` of `$$` (the delimiter of mysql client scripts, as in `END$$`). */
function mysqlWordAt(sql: string, i: number) {
  return i < sql.length && MYSQL_WORD.test(sql[i]) && !(sql[i] === "$" && sql[i + 1] === "$");
}

function mysqlLineEnd(sql: string, i: number) {
  const end = sql.indexOf("\n", i);
  return end === -1 ? sql.length : end;
}

/** The next token at or after `i`, past whitespace and comments. */
function mysqlToken(sql: string, i: number): MysqlToken | null {
  while (i < sql.length) {
    const ch = sql[i];
    const next = sql[i + 1];
    if (/\s/.test(ch)) i++;
    else if (ch === "#") i = mysqlLineEnd(sql, i);
    // `--` is a comment only before whitespace: `1--1` is one minus minus one
    else if (ch === "-" && next === "-" && (i + 2 >= sql.length || /\s/.test(sql[i + 2]))) i = mysqlLineEnd(sql, i);
    else if (ch === "/" && next === "*") {
      // Block comments don't nest; `/*! … */` is run by MySQL, so it's part of a statement
      const close = sql.indexOf("*/", i + 2);
      const end = close === -1 ? sql.length : close + 2;
      if (sql[i + 2] === "!") return { kind: "versioned", text: "", start: i, end };
      i = end;
    } else if (ch === "'" || ch === '"' || ch === "`") {
      let j = i + 1;
      while (j < sql.length) {
        if (sql[j] === "\\" && ch !== "`") j += 2;
        else if (sql[j] === ch && sql[j + 1] === ch) j += 2;
        else if (sql[j] === ch) {
          j++;
          break;
        } else j++;
      }
      return { kind: "quoted", text: "", start: i, end: Math.min(j, sql.length) };
    } else if (ch === ";") return { kind: "semi", text: ";", start: i, end: i + 1 };
    else if (mysqlWordAt(sql, i)) {
      let end = i + 1;
      while (mysqlWordAt(sql, end)) end++;
      return { kind: "word", text: sql.slice(i, end).replace(/[a-z]+/g, (w) => w.toUpperCase()), start: i, end };
    } else return { kind: "punct", text: ch, start: i, end: i + 1 };
  }
  return null;
}

/**
 * Whether the first tokens of a statement are `CREATE [OR REPLACE] [DEFINER = user] [AGGREGATE]
 * PROCEDURE|FUNCTION|TRIGGER|EVENT`; null while there are too few to tell.
 */
function mysqlRoutineHeader(tokens: MysqlToken[]): boolean | null {
  const word = (i: number) => (tokens[i]?.kind === "word" ? tokens[i].text : null);
  const name = (i: number) => tokens[i]?.kind === "word" || tokens[i]?.kind === "quoted";
  const punct = (i: number, c: string) => tokens[i]?.kind === "punct" && tokens[i].text === c;
  const missing = (i: number) => i >= tokens.length;

  if (missing(0)) return null;
  if (word(0) !== "CREATE") return false;
  let i = 1;
  if (missing(i)) return null;
  if (word(i) === "OR") {
    if (missing(i + 1)) return null;
    if (word(i + 1) !== "REPLACE") return false;
    i += 2;
  }
  if (missing(i)) return null;
  if (word(i) === "DEFINER") {
    if (missing(i + 1)) return null;
    if (!punct(i + 1, "=")) return false;
    i += 2;
    if (missing(i)) return null;
    if (word(i) === "CURRENT_USER") {
      i++;
      if (missing(i)) return null;
      if (punct(i, "(")) {
        if (missing(i + 1)) return null;
        if (!punct(i + 1, ")")) return false;
        i += 2;
      }
    } else {
      // user, or user@host, each a name or a quoted string
      if (!name(i)) return false;
      i++;
      if (missing(i)) return null;
      if (punct(i, "@")) {
        if (missing(i + 1)) return null;
        if (!name(i + 1)) return false;
        i += 2;
      }
    }
  }
  if (missing(i)) return null;
  if (word(i) === "AGGREGATE") i++;
  if (missing(i)) return null;
  const kind = word(i);
  return kind === "PROCEDURE" || kind === "FUNCTION" || kind === "TRIGGER" || kind === "EVENT";
}

const MYSQL_END_KEYWORDS = new Set(["IF", "CASE", "LOOP", "WHILE", "REPEAT"]);
const MYSQL_OPERAND_WORDS = new Set(["WHEN", "THEN", "ELSE", "CASE", "AND", "OR", "NOT"]);

/** Before an END that closes a CASE expression comes a value; after these an `end` is a column name. */
function mysqlExpectsOperand(token: MysqlToken | null) {
  if (token?.kind === "punct") return ",(=<>+-*/%!&|^~".includes(token.text);
  return token?.kind === "word" && MYSQL_OPERAND_WORDS.has(token.text);
}

/**
 * MySQL's statements end at a semicolon, except in the body of a CREATE PROCEDURE/FUNCTION/
 * TRIGGER/EVENT, which only ends once the blocks it opened (BEGIN, CASE, and IF, LOOP, WHILE and
 * REPEAT starting a statement of the body) have closed. A `DELIMITER …` line is a statement of its
 * own: the backend answers it with an error. Same rules as `db/mysql_common/split.rs`, which runs
 * the same cases (split_cases.json).
 */
function splitMysql(sql: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  const push = (from: number, to: number) => {
    while (from < to && /\s/.test(sql[from])) from++;
    while (to > from && /\s/.test(sql[to - 1])) to--;
    if (to > from) ranges.push({ from, to });
  };
  const fresh = (start: number) => ({
    start,
    hasContent: false,
    header: [] as MysqlToken[],
    routine: null as boolean | null,
    blocks: [] as MysqlBlock[],
    // The next word starts a statement of the body (after `;`, BEGIN, THEN, a label…)
    bodyStart: false,
    // The statement of the body is a DECLARE, whose HANDLER may be a BEGIN … END block
    inDeclare: false,
    // The body opened a block and all its blocks have closed again
    bodyClosed: false,
    previous: null as MysqlToken | null,
  });
  let current = fresh(0);
  const open = (block: MysqlBlock) => {
    current.blocks.push(block);
    current.inDeclare = false;
  };
  const close = () => {
    current.blocks.pop();
    if (current.blocks.length === 0) current.bodyClosed = true;
  };
  const isPunct = (token: MysqlToken | null, c: string) => token?.kind === "punct" && token.text === c;

  let i = 0;
  for (let token = mysqlToken(sql, i); token; token = mysqlToken(sql, i)) {
    i = token.end;

    if (token.kind === "semi") {
      if (current.routine === true && current.blocks.length > 0) {
        // A statement of the body ends, the CREATE goes on
        current.bodyStart = true;
        current.inDeclare = false;
        current.previous = token;
        continue;
      }
      if (current.hasContent) push(current.start, token.start);
      current = fresh(token.end);
      continue;
    }

    if (token.kind === "word" && token.text === "DELIMITER" && (token.end >= sql.length || /\s/.test(sql[token.end]))) {
      const lineStart = sql.lastIndexOf("\n", token.start - 1) + 1;
      const firstOnLine = !sql.slice(lineStart, token.start).trim();
      // At the start of a statement, or on the line after a body that closed (`END$$`)
      const afterBody = current.routine === true && current.bodyClosed && current.blocks.length === 0 && firstOnLine;
      if (!current.hasContent || afterBody) {
        let from = current.start;
        if (current.hasContent) {
          push(current.start, lineStart);
          from = lineStart;
        }
        const end = mysqlLineEnd(sql, token.end);
        push(from, end);
        current = fresh(end);
        i = end;
        continue;
      }
    }

    current.hasContent = true;
    if (current.routine === null) {
      current.header.push(token);
      current.routine = mysqlRoutineHeader(current.header);
      current.previous = token;
      continue;
    }
    if (!current.routine || token.kind !== "word") {
      current.bodyStart = false;
      current.previous = token;
      continue;
    }

    const bodyStart = current.bodyStart;
    current.bodyStart = false;
    const word = token.text;
    const peek = mysqlToken(sql, i);
    const nextWord = peek?.kind === "word" ? peek.text : null;
    const previous = current.previous;
    // `t.end`, `@end`: a name, never a keyword
    const named = isPunct(previous, ".") || isPunct(previous, "@");

    // `label:` (not `:=`) is followed by a statement of the body
    if (peek && isPunct(peek, ":") && sql[peek.end] !== "=") {
      i = peek.end;
      current.bodyStart = true;
      current.previous = peek;
      continue;
    }

    if (named) {
      // nothing: a name
    } else if (word === "BEGIN") {
      const inAList = isPunct(previous, ",") || isPunct(previous, "(");
      const transaction = nextWord === "WORK" || peek?.kind === "semi";
      if (!inAList && !transaction && (bodyStart || current.blocks.length === 0 || current.inDeclare)) {
        open("begin");
        current.bodyStart = true;
      }
    } else if (word === "CASE") open(bodyStart ? "caseStatement" : "caseExpression");
    else if (word === "IF" && bodyStart) open("if");
    else if (word === "WHILE" && bodyStart) open("while");
    else if ((word === "LOOP" || word === "REPEAT") && bodyStart) {
      open(word === "LOOP" ? "loop" : "repeat");
      current.bodyStart = true;
    } else if (word === "END") {
      const innermost = current.blocks[current.blocks.length - 1];
      const endKeyword = nextWord !== null && MYSQL_END_KEYWORDS.has(nextWord);
      // A CASE expression closes after a value; a block after a statement of the body, or at `END IF`
      const closes =
        innermost === undefined ? false : innermost === "caseExpression" ? !mysqlExpectsOperand(previous) : bodyStart || endKeyword;
      if (closes) {
        close();
        if (endKeyword && peek) {
          i = peek.end;
          current.previous = peek;
          continue;
        }
      }
    } else if (word === "THEN" || word === "ELSE") {
      const innermost = current.blocks[current.blocks.length - 1];
      if (innermost === "if" || innermost === "caseStatement") current.bodyStart = true;
    }
    // `WHILE … DO` and an event's `DO`; a DO that starts a statement is the DO statement
    else if (word === "DO" && !bodyStart) current.bodyStart = true;
    else if (word === "DECLARE" && bodyStart) current.inDeclare = true;
    // A trigger's body comes after FOR EACH ROW [FOLLOWS|PRECEDES other]
    else if (word === "ROW" && current.blocks.length === 0 && previous?.kind === "word" && previous.text === "EACH") current.bodyStart = true;
    else if ((word === "FOLLOWS" || word === "PRECEDES") && bodyStart && current.blocks.length === 0 && peek) {
      i = peek.end;
      current.bodyStart = true;
      current.previous = peek;
      continue;
    }
    current.previous = token;
  }
  if (current.hasContent) push(current.start, sql.length);
  return ranges;
}

function splitPostgres(sql: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  let start = 0;
  let i = 0;
  const push = (end: number) => {
    let from = start;
    let to = end;
    while (from < to && /\s/.test(sql[from])) from++;
    while (to > from && /\s/.test(sql[to - 1])) to--;
    // A statement that is only comments still runs as nothing; keep it out
    if (to > from && sql.slice(from, to).replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, "").trim()) ranges.push({ from, to });
  };

  while (i < sql.length) {
    const ch = sql[i];
    const next = sql[i + 1];
    if (ch === "-" && next === "-") {
      const end = sql.indexOf("\n", i);
      i = end === -1 ? sql.length : end + 1;
    } else if (ch === "/" && next === "*") {
      let depth = 1;
      i += 2;
      while (i < sql.length && depth > 0) {
        if (sql[i] === "/" && sql[i + 1] === "*") {
          depth++;
          i += 2;
        } else if (sql[i] === "*" && sql[i + 1] === "/") {
          depth--;
          i += 2;
        } else i++;
      }
    } else if (ch === "'" || ch === '"') {
      // E'...' strings treat backslash as an escape
      const escapes = ch === "'" && /[eE]/.test(sql[i - 1] ?? "") && !/[\w$]/.test(sql[i - 2] ?? "");
      i++;
      while (i < sql.length) {
        if (escapes && sql[i] === "\\") i += 2;
        else if (sql[i] === ch && sql[i + 1] === ch) i += 2;
        else if (sql[i] === ch) {
          i++;
          break;
        } else i++;
      }
    } else if (ch === "$") {
      const tag = sql.slice(i).match(/^\$([A-Za-z_][A-Za-z0-9_]*)?\$/);
      // "$1" is a parameter, not a dollar quote
      if (tag && !/[\w]/.test(sql[i - 1] ?? "")) {
        const end = sql.indexOf(tag[0], i + tag[0].length);
        i = end === -1 ? sql.length : end + tag[0].length;
      } else i++;
    } else if (ch === ";") {
      push(i);
      start = i + 1;
      i++;
    } else i++;
  }
  push(sql.length);
  return ranges;
}

/**
 * The statement the cursor is in. On blank space between statements it's the one
 * before (the cursor usually sits right after its semicolon), or the first one.
 */
export function statementAt(ranges: StatementRange[], pos: number): StatementRange | null {
  if (ranges.length === 0) return null;
  let candidate: StatementRange | null = null;
  for (const range of ranges) {
    // +1 counts the semicolon right after the statement
    if (pos >= range.from && pos <= range.to + 1) return range;
    if (range.to < pos) candidate = range;
  }
  return candidate ?? ranges[0];
}
