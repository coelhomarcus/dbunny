/** Where one statement of a script starts and ends (without the closing semicolon). */
export interface StatementRange {
  from: number;
  to: number;
}

/** The SQL a connection speaks, which decides where its statements end. */
export type SqlDialectName = "postgres" | "sqlite";

/**
 * Splits a script at the semicolons that end statements, skipping the ones inside
 * strings, quoted names and comments. PostgreSQL also has E'...' strings, nested block
 * comments and dollar-quoted bodies ($$...$$, $tag$...$tag$); SQLite has `...` and [...]
 * names and triggers whose body holds statements. Empty statements are dropped.
 */
export function splitStatements(sql: string, dialect: SqlDialectName = "postgres"): StatementRange[] {
  return dialect === "sqlite" ? splitSqlite(sql) : splitPostgres(sql);
}

/** Adds the statement between `from` and `to` (trimmed) unless it's empty or only comments. */
function pushStatement(ranges: StatementRange[], sql: string, from: number, to: number) {
  while (from < to && /\s/.test(sql[from])) from++;
  while (to > from && /\s/.test(sql[to - 1])) to--;
  // A statement that is only comments still runs as nothing; keep it out
  if (to > from && sql.slice(from, to).replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, "").trim()) ranges.push({ from, to });
}

/** True for `CREATE [TEMP|TEMPORARY] TRIGGER`, given the first words of a statement. */
function startsTrigger(words: string[]) {
  return words[0] === "CREATE" && (words[1] === "TRIGGER" || ((words[1] === "TEMP" || words[1] === "TEMPORARY") && words[2] === "TRIGGER"));
}

function splitSqlite(sql: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  let start = 0;
  let i = 0;
  // The first words of the current statement, to know when it's a trigger
  let words: string[] = [];
  // Inside a trigger's body (BEGIN ... END), and inside a CASE ... END within it
  let depth = 0;

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
      if (words.length < 3) words.push(ch);
    } else if (ch === "[") {
      const end = sql.indexOf("]", i + 1);
      i = end === -1 ? sql.length : end + 1;
      if (words.length < 3) words.push("[");
    } else if (/[A-Za-z_]/.test(ch)) {
      let end = i + 1;
      while (end < sql.length && /[A-Za-z0-9_$]/.test(sql[end])) end++;
      const word = sql.slice(i, end).toUpperCase();
      if (words.length < 3) words.push(word);
      if (startsTrigger(words)) {
        if (word === "BEGIN" || word === "CASE") depth++;
        else if (word === "END" && depth > 0) depth--;
      }
      i = end;
    } else if (ch === ";" && depth === 0) {
      pushStatement(ranges, sql, start, i);
      start = i + 1;
      words = [];
      i++;
    } else i++;
  }
  pushStatement(ranges, sql, start, sql.length);
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
