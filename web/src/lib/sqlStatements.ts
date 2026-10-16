/** Where one statement of a script starts and ends (without the closing semicolon). */
export interface StatementRange {
  from: number;
  to: number;
}

/**
 * Splits a script at the semicolons that end statements, skipping the ones inside
 * strings ('...', E'...'), quoted names ("..."), comments (-- and nested /* *\/)
 * and dollar-quoted bodies ($$...$$, $tag$...$tag$). Empty statements are dropped.
 */
export function splitStatements(sql: string): StatementRange[] {
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
