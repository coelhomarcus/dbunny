import type { SqlFeature } from "@/types";
import { splitStatements, type SqlDialectName } from "../../sqlStatements.ts";
import type { SqlDialect } from "../dialects/types.ts";

/** A table, view or CTE the statement reads, as written. */
export interface QueryRelation {
  schema: string | null;
  name: string;
  alias: string | null;
  /** How the query refers to it (its alias, else its name), exactly as typed: `u`, `Users`, `"Orders"`. */
  written: string;
}

/** A `WITH` query and the column names it declares, if it lists them. */
export interface QueryCte {
  name: string;
  columns: string[];
}

/** What the cursor is in the middle of; `quiet` is a string, a comment or a name being made up. */
export type Position =
  | { kind: "table"; afterJoin: boolean }
  | { kind: "joinCondition"; target: QueryRelation }
  /** After `qualifier.`; `schema` when it was written `schema.qualifier.` */
  | { kind: "qualified"; qualifier: string; schema: string | null }
  | { kind: "column" }
  | { kind: "insertColumns"; table: QueryRelation; present: string[] }
  | { kind: "statementStart" }
  /** Right after a complete expression or relation: the next word is a keyword (`WHERE`, `JOIN`, `AS`). */
  | { kind: "keyword" }
  | { kind: "quiet" };

export interface QueryContext {
  relations: QueryRelation[];
  ctes: QueryCte[];
  position: Position;
  word: { from: number; text: string };
}

type TokenType = "word" | "quoted" | "string" | "number" | "comment" | "punct";

interface Token {
  type: TokenType;
  text: string;
  /** A name without its quotes; a word as written. */
  value: string;
  from: number;
  to: number;
  /** Whether the string, quoted name or block comment reached its closing mark. */
  closed: boolean;
  /** The parentheses the token sits in, by where each opened: "/12/40/". */
  scope: string;
}

const WORD_START = /[A-Za-z_\u0080-￿]/;
const WORD_CHAR = /[A-Za-z0-9_$\u0080-￿]/;

/** Splits SQL into tokens the way `dialect` reads it: what is a string, a name or a comment. */
function tokenize(sql: string, offset: number, dialect: SqlDialect): Token[] {
  const tokens: Token[] = [];
  const mysqlFamily = dialect.identifierQuote === "`";
  const postgres = dialect.engine === "postgres";
  const nameQuotes = dialect.engine === "sqlite" ? ['"', "`", "["] : [dialect.identifierQuote];
  const stack: number[] = [];
  const scope = () => (stack.length ? `/${stack.join("/")}/` : "/");
  let i = 0;
  const push = (type: TokenType, start: number, end: number, closed = true, value?: string) => {
    const text = sql.slice(start, end);
    tokens.push({ type, text, value: value ?? text, from: offset + start, to: offset + end, closed, scope: scope() });
  };
  // A quoted run that doubles its closing mark to escape it, and (backslash) also takes \x
  const quoted = (start: number, close: string, backslash: boolean) => {
    let j = start + 1;
    while (j < sql.length) {
      const c = sql[j];
      if (backslash && c === "\\") j += 2;
      else if (c === close) {
        if (sql[j + 1] === close && close !== "]") j += 2;
        else return { end: j + 1, closed: true };
      } else j++;
    }
    return { end: sql.length, closed: false };
  };
  while (i < sql.length) {
    const c = sql[i];
    const rest = sql.slice(i);
    if (/\s/.test(c)) {
      i++;
      continue;
    }
    const line = dialect.lineComments.find((mark) => rest.startsWith(mark));
    if (line) {
      const end = sql.indexOf("\n", i);
      push("comment", i, end < 0 ? sql.length : end, false);
      i = end < 0 ? sql.length : end;
      continue;
    }
    if (rest.startsWith("/*")) {
      let depth = 0;
      let j = i;
      let closed = false;
      while (j < sql.length) {
        if (sql.startsWith("/*", j)) {
          depth = postgres || depth === 0 ? depth + 1 : depth;
          j += 2;
        } else if (sql.startsWith("*/", j)) {
          depth--;
          j += 2;
          if (depth === 0) {
            closed = true;
            break;
          }
        } else j++;
      }
      push("comment", i, j, closed);
      i = j;
      continue;
    }
    if (postgres && c === "$") {
      const tag = /^\$([A-Za-z_][A-Za-z0-9_]*)?\$/.exec(rest);
      if (tag) {
        const close = sql.indexOf(tag[0], i + tag[0].length);
        const end = close < 0 ? sql.length : close + tag[0].length;
        push("string", i, end, close >= 0);
        i = end;
        continue;
      }
    }
    // E'…' takes backslash escapes in PostgreSQL; N'…', X'…' and B'…' are strings too
    if (/[EeNnXxBb]/.test(c) && sql[i + 1] === "'" && !(i > 0 && WORD_CHAR.test(sql[i - 1]))) {
      const { end, closed } = quoted(i + 1, "'", mysqlFamily || (postgres && /[Ee]/.test(c)));
      push("string", i, end, closed);
      i = end;
      continue;
    }
    if (c === "'" || (mysqlFamily && c === '"')) {
      const { end, closed } = quoted(i, c, mysqlFamily);
      push("string", i, end, closed);
      i = end;
      continue;
    }
    if (nameQuotes.includes(c)) {
      const close = c === "[" ? "]" : c;
      const { end, closed } = quoted(i, close, false);
      const inner = sql.slice(i + 1, closed ? end - 1 : end);
      push("quoted", i, end, closed, close === "]" ? inner : inner.split(close + close).join(close));
      i = end;
      continue;
    }
    if (WORD_START.test(c)) {
      let j = i + 1;
      while (j < sql.length && WORD_CHAR.test(sql[j])) j++;
      push("word", i, j);
      i = j;
      continue;
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      while (j < sql.length && /[0-9.eE]/.test(sql[j])) j++;
      push("number", i, j);
      i = j;
      continue;
    }
    if (c === "(") {
      push("punct", i, i + 1);
      stack.push(offset + i);
    } else if (c === ")") {
      stack.pop();
      push("punct", i, i + 1);
    } else push("punct", i, i + 1);
    i++;
  }
  return tokens;
}

/** Where the statement under the cursor starts and ends, cut the way the editor runs statements. */
function statementBounds(doc: string, pos: number, dialect: SqlDialect) {
  const name: SqlDialectName = dialect.engine === "postgres" ? "postgres" : dialect.engine === "sqlite" ? "sqlite" : "mysql";
  let from = 0;
  for (const range of splitStatements(doc, name)) {
    if (range.from > pos) return { from, to: Math.max(pos, from) };
    if (pos <= range.to) return { from: range.from, to: range.to };
    // Past this statement: still in it until its semicolon
    from = doc[range.to] === ";" ? range.to + 1 : range.from;
  }
  return { from, to: doc.length };
}

const CLAUSES = new Set([
  "SELECT", "FROM", "JOIN", "ON", "WHERE", "SET", "BY", "HAVING", "RETURNING", "UPDATE", "INTO",
  "TRUNCATE", "VALUES", "LIMIT", "OFFSET", "WITH", "DELETE", "INSERT", "USING",
]);
const RELATION_CLAUSES = new Set(["FROM", "JOIN", "UPDATE", "INTO", "TRUNCATE"]);
const COLUMN_CLAUSES = new Set(["SELECT", "WHERE", "SET", "BY", "HAVING", "ON", "RETURNING"]);
/** Words after which an expression continues, so a column comes next. */
const OPERATOR_WORDS = new Set([
  "AND", "OR", "NOT", "CASE", "WHEN", "THEN", "ELSE", "LIKE", "ILIKE", "IN", "IS", "BETWEEN",
  "DISTINCT", "ALL", "ANY", "SOME", "EXISTS", "REGEXP", "RLIKE", "GLOB", "MATCH", "ESCAPE",
]);
/** Clause words that MySQL and MariaDB also have as functions: `TRUNCATE(x, 2)`, `VALUES(col)`, `INSERT(s, …)`. */
const ALSO_FUNCTIONS = new Set(["TRUNCATE", "VALUES", "INSERT"]);
/** Words that end a relation reference instead of naming its alias. */
const NOT_ALIAS = new Set([
  "ON", "USING", "WHERE", "JOIN", "INNER", "LEFT", "RIGHT", "FULL", "CROSS", "NATURAL", "OUTER",
  "STRAIGHT_JOIN", "GROUP", "ORDER", "HAVING", "LIMIT", "OFFSET", "UNION", "EXCEPT", "INTERSECT",
  "WINDOW", "SET", "VALUES", "SELECT", "RETURNING", "FOR", "LATERAL", "DEFAULT", "AS", "WITH",
  "PARTITION", "TABLESAMPLE", "FETCH", "INDEXED", "NOT", "USE", "FORCE", "IGNORE", "QUALIFY",
]);

const upper = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isName = (token: Token | undefined) => token?.type === "word" || token?.type === "quoted";

interface Parsed {
  relation: QueryRelation;
  /** Index of the clause word or comma that introduced it. */
  at: number;
  scope: string;
}

/**
 * Reads `[schema.]name [[AS] alias]` from `tokens[i]`; returns the relation and where it ended.
 * `columnList` says a `(` after the name opens its columns (`INSERT INTO t (a, b)`), not a call.
 */
function readRelation(tokens: Token[], i: number, columnList = false) {
  if (!isName(tokens[i])) return null;
  let schema: string | null = null;
  let name = tokens[i].value;
  let j = i + 1;
  if (tokens[j]?.text === "." && isName(tokens[j + 1])) {
    schema = name;
    name = tokens[j + 1].value;
    j += 2;
  }
  // A function in FROM (`generate_series(…)`) isn't a relation the schema knows
  if (tokens[j]?.text === "(" && tokens[j].scope === tokens[i].scope) {
    if (!columnList) return null;
    return { relation: { schema, name, alias: null, written: tokens[j - 1].text }, end: j };
  }
  let alias: string | null = null;
  let written = tokens[j - 1].text;
  if (upper(tokens[j]) === "AS" && isName(tokens[j + 1])) {
    alias = tokens[j + 1].value;
    written = tokens[j + 1].text;
    j += 2;
  } else if (tokens[j]?.type === "quoted" || (tokens[j]?.type === "word" && !NOT_ALIAS.has(upper(tokens[j])))) {
    alias = tokens[j].value;
    written = tokens[j].text;
    j += 1;
  }
  return { relation: { schema, name, alias, written }, end: j };
}

function readRelations(tokens: Token[]): Parsed[] {
  const out: Parsed[] = [];
  for (let i = 0; i < tokens.length; i++) {
    let keyword = upper(tokens[i]);
    let start = i + 1;
    if (keyword === "TRUNCATE" && upper(tokens[start]) === "TABLE") start++;
    if (keyword === "UPDATE" && /^(LOW_PRIORITY|IGNORE|ONLY)$/.test(upper(tokens[start]))) start++;
    if (!RELATION_CLAUSES.has(keyword)) continue;
    // ON DUPLICATE KEY UPDATE, ON CONFLICT … DO UPDATE and SELECT … INTO aren't relations
    if (keyword === "UPDATE" && /^(KEY|DO)$/.test(upper(tokens[i - 1]))) continue;
    let read = readRelation(tokens, start, keyword === "INTO");
    while (read) {
      out.push({ relation: read.relation, at: i, scope: tokens[start].scope });
      if (keyword !== "FROM" || tokens[read.end]?.text !== ",") break;
      i = read.end;
      start = read.end + 1;
      keyword = "FROM";
      read = readRelation(tokens, start);
    }
  }
  return out;
}

function readCtes(tokens: Token[]): QueryCte[] {
  const first = tokens.findIndex((t) => upper(t) === "WITH");
  if (first !== 0) return [];
  const ctes: QueryCte[] = [];
  let i = upper(tokens[1]) === "RECURSIVE" ? 2 : 1;
  while (isName(tokens[i])) {
    const name = tokens[i].value;
    const columns: string[] = [];
    i++;
    if (tokens[i]?.text === "(") {
      for (i++; i < tokens.length && tokens[i].text !== ")"; i++) if (isName(tokens[i])) columns.push(tokens[i].value);
      i++;
    }
    if (upper(tokens[i]) !== "AS") break;
    i++;
    while (/^(NOT|MATERIALIZED)$/.test(upper(tokens[i]))) i++;
    if (tokens[i]?.text !== "(") break;
    ctes.push({ name, columns });
    const scope = tokens[i].scope;
    for (i++; i < tokens.length && !(tokens[i].text === ")" && tokens[i].scope === scope); i++);
    i++;
    if (tokens[i]?.text !== ",") break;
    i++;
  }
  return ctes;
}

function isInside(token: Token, pos: number) {
  if (pos <= token.from) return false;
  return pos < token.to || (pos === token.to && !token.closed);
}

/** Reads the statement under the cursor: what it reads from, and what can be written at `pos`. */
export function analyze(doc: string, pos: number, dialect: SqlDialect, features: SqlFeature[]): QueryContext {
  const bounds = statementBounds(doc, pos, dialect);
  const all = tokenize(doc.slice(bounds.from, Math.max(bounds.to, pos)), bounds.from, dialect);
  const tokens = all.filter((t) => t.type !== "comment");
  const ctes = readCtes(tokens);
  const quiet = (word = { from: pos, text: "" }): QueryContext => ({ relations: [], ctes, position: { kind: "quiet" }, word });

  const around = all.find((t) => isInside(t, pos));
  if (around && (around.type === "string" || around.type === "comment")) return quiet();
  let word = { from: pos, text: "" };
  if (around?.type === "quoted") word = { from: around.from, text: doc.slice(around.from, pos) };
  else {
    let from = pos;
    while (from > bounds.from && WORD_CHAR.test(doc[from - 1])) from--;
    word = { from, text: doc.slice(from, pos) };
  }

  const before = tokens.filter((t) => t.to <= word.from);
  const cursorScope = before.length === 0 ? "/" : scopeAt(before);
  const parsed = readRelations(tokens).filter((p) => cursorScope.startsWith(p.scope));
  const relations = parsed.map((p) => p.relation);
  const context = (position: Position): QueryContext => ({ relations, ctes, position, word });

  if (before.length === 0) return context({ kind: "statementStart" });
  const last = before[before.length - 1];
  if (last.text === "." && isName(before[before.length - 2])) {
    const schema = before[before.length - 3]?.text === "." && isName(before[before.length - 4]) ? before[before.length - 4].value : null;
    return context({ kind: "qualified", qualifier: before[before.length - 2].value, schema });
  }

  // The nearest clause at the cursor's level, or the parenthesis the cursor is inside
  for (let i = before.length - 1; i >= 0; i--) {
    const token = before[i];
    if (token.text === "(" && `${token.scope}${token.from}/` === cursorScope) {
      return context(insideParenthesis(before, i, relations, word.from));
    }
    if (token.scope !== cursorScope) continue;
    let keyword = upper(token);
    if (!CLAUSES.has(keyword)) continue;
    if (ALSO_FUNCTIONS.has(keyword) && before[i + 1]?.text === "(") continue;
    // ON DUPLICATE KEY UPDATE and DO UPDATE set columns, like SET
    if (keyword === "UPDATE" && /^(KEY|DO)$/.test(upper(before[i - 1]))) keyword = "SET";
    const lastWord = upper(last);
    if (RELATION_CLAUSES.has(keyword)) {
      const fresh = last === token || (keyword === "TRUNCATE" && lastWord === "TABLE") || (keyword === "FROM" && last.text === ",");
      if (fresh || (keyword === "UPDATE" && /^(LOW_PRIORITY|IGNORE|ONLY)$/.test(lastWord))) {
        return context({ kind: "table", afterJoin: keyword === "JOIN" });
      }
      return context(lastWord === "AS" ? { kind: "quiet" } : { kind: "keyword" });
    }
    if (keyword === "ON" && last === token) {
      // `before` is the start of `tokens`, so their indexes agree
      const target = [...parsed].reverse().find((p) => p.at < i && upper(tokens[p.at]) === "JOIN");
      if (target) return context({ kind: "joinCondition", target: target.relation });
    }
    if (COLUMN_CLAUSES.has(keyword)) {
      if (keyword === "RETURNING" && !features.includes("returning")) return context({ kind: "keyword" });
      const continues = last === token || last.type === "punct" || OPERATOR_WORDS.has(lastWord);
      if (lastWord === "AS") return context({ kind: "quiet" });
      return context(continues && last.text !== ")" ? { kind: "column" } : { kind: "keyword" });
    }
    if (keyword === "WITH") return context(last === token || last.text === "," ? { kind: "quiet" } : { kind: "keyword" });
    return context({ kind: "keyword" });
  }
  return context({ kind: "keyword" });
}

/** The scope a token written after `before` gets: inside the parenthesis the last token opened, if it did. */
function scopeAt(before: Token[]) {
  const last = before[before.length - 1];
  return last.text === "(" ? `${last.scope}${last.from}/` : last.scope;
}

/** Inside an open parenthesis: an INSERT column list, the body of a CTE, or an expression. */
function insideParenthesis(before: Token[], open: number, relations: QueryRelation[], wordFrom: number): Position {
  const prev = before[open - 1];
  if (upper(prev) === "AS" || /^(MATERIALIZED)$/.test(upper(prev))) return { kind: "statementStart" };
  // INSERT INTO [schema.]table (
  let j = open - 1;
  if (isName(before[j])) {
    let schema: string | null = null;
    const name = before[j].value;
    const written = before[j].text;
    if (before[j - 1]?.text === "." && isName(before[j - 2])) {
      schema = before[j - 2].value;
      j -= 2;
    }
    if (upper(before[j - 1]) === "INTO") {
      const scope = before[open].scope + `${before[open].from}/`;
      const present = before.slice(open + 1).filter((t) => t.scope === scope && isName(t) && t.to <= wordFrom).map((t) => t.value);
      const table = relations.find((r) => r.name === name && r.schema === schema) ?? { schema, name, alias: null, written };
      return { kind: "insertColumns", table: { ...table, alias: null }, present };
    }
  }
  return { kind: "column" };
}
