/** Splits CSV text into rows of fields (RFC 4180: quotes, doubled quotes, line breaks inside quotes). */
export function parseCsv(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  // A byte order mark would end up in the first header
  const input = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;

  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (quoted) {
      if (ch === '"') {
        if (input[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          quoted = false;
        }
      } else {
        field += ch;
      }
    } else if (ch === '"' && field === "") {
      quoted = true;
    } else if (ch === delimiter) {
      row.push(field);
      field = "";
    } else if (ch === "\n" || ch === "\r") {
      if (ch === "\r" && input[i + 1] === "\n") i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else {
      field += ch;
    }
  }
  if (field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  // Blank lines (often a trailing one) aren't rows
  return rows.filter((r) => !(r.length === 1 && r[0] === ""));
}

/** The delimiter that splits the first line into the most fields: comma, semicolon, tab or pipe. */
export function detectDelimiter(text: string): string {
  const firstLine = text.split(/\r?\n/, 1)[0] ?? "";
  const candidates = [",", ";", "\t", "|"];
  const count = (d: string) => parseCsv(firstLine, d)[0]?.length ?? 0;
  return candidates.reduce((best, d) => (count(d) > count(best) ? d : best), ",");
}

/** Matches CSV headers to table columns by name, ignoring case, spaces and underscores. */
export function matchColumns(headers: string[], columns: string[]): (string | null)[] {
  const key = (s: string) => s.toLowerCase().replace(/[\s_-]+/g, "");
  return headers.map((h) => columns.find((c) => c === h) ?? columns.find((c) => key(c) === key(h)) ?? null);
}
