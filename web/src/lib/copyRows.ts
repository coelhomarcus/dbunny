import type { QueryColumn } from "@/types";

function text(value: unknown): string {
  if (value === null || value === undefined) return "";
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

/** One value as plain text, the way it's shown in the cell (NULL becomes "NULL"). */
export function valueText(value: unknown): string {
  return value === null || value === undefined ? "NULL" : text(value);
}

/** Tab-separated with a header row, quoted like spreadsheets expect when a value has tabs, quotes or line breaks. */
export function rowsToTsv(columns: QueryColumn[], rows: unknown[][]): string {
  const field = (v: unknown) => {
    const s = text(v);
    return /[\t\n\r"]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return [columns.map((c) => field(c.name)), ...rows.map((row) => row.map(field))].map((r) => r.join("\t")).join("\n");
}

/** An object per row; a single row is copied as one object rather than a list. */
export function rowsToJson(columns: QueryColumn[], rows: unknown[][]): string {
  const objects = rows.map((row) => Object.fromEntries(columns.map((c, i) => [c.name, row[i] ?? null])));
  return JSON.stringify(objects.length === 1 ? objects[0] : objects, null, 2);
}

export function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

export function sqlLiteral(value: unknown): string {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "number" || typeof value === "bigint") return String(value);
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  // Everything else is a quoted literal; Postgres casts it to the column's type
  return `'${text(value).replace(/'/g, "''")}'`;
}

/** A single INSERT with one VALUES row per row. */
export function rowsToInsertSql(schema: string, table: string, columns: QueryColumn[], rows: unknown[][]): string {
  const target = `${quoteIdent(schema)}.${quoteIdent(table)}`;
  const names = columns.map((c) => quoteIdent(c.name)).join(", ");
  const values = rows.map((row) => `  (${row.map(sqlLiteral).join(", ")})`).join(",\n");
  return `INSERT INTO ${target} (${names}) VALUES\n${values};`;
}

/** The UPDATE statements that saving pending edits will run, one per changed row. */
export function pendingChangesToSql(
  schema: string,
  table: string,
  changes: { key: Record<string, unknown>; updates: Record<string, unknown> }[],
): string {
  const target = `${quoteIdent(schema)}.${quoteIdent(table)}`;
  return changes
    .map(({ key, updates }) => {
      const set = Object.entries(updates).map(([column, value]) => `${quoteIdent(column)} = ${sqlLiteral(value)}`);
      const where = Object.entries(key).map(([column, value]) => `${quoteIdent(column)} = ${sqlLiteral(value)}`);
      return `UPDATE ${target}\nSET ${set.join(",\n    ")}\nWHERE ${where.join(" AND ")};`;
    })
    .join("\n\n");
}
