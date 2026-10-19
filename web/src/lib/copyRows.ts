import type { QueryColumn } from "@/types";
// With the extension, so Node's test runner can load it too
import type { SqlDialectName } from "./sqlStatements.ts";

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

/** Quotes a name for a dialect: backticks for MySQL, double quotes for the others. */
export function quoteIdentFor(dialect: SqlDialectName): (name: string) => string {
  return dialect === "mysql" ? (name) => `\`${name.replace(/`/g, "``")}\`` : (name) => `"${name.replace(/"/g, '""')}"`;
}

/**
 * A value as a SQL literal in a dialect. Everything that isn't a number or a boolean is a quoted
 * literal that the server converts to the column's type; MySQL reads a backslash in it as an
 * escape, so it is doubled.
 */
export function sqlLiteralFor(dialect: SqlDialectName): (value: unknown) => string {
  const escape = dialect === "mysql" ? (s: string) => s.replace(/\\/g, "\\\\").replace(/'/g, "''") : (s: string) => s.replace(/'/g, "''");
  return (value) => {
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "number" || typeof value === "bigint") return String(value);
    if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
    return `'${escape(text(value))}'`;
  };
}

/** A single INSERT with one VALUES row per row. */
export function rowsToInsertSql(schema: string, table: string, columns: QueryColumn[], rows: unknown[][], dialect: SqlDialectName = "postgres"): string {
  const quote = quoteIdentFor(dialect);
  const literal = sqlLiteralFor(dialect);
  const target = `${quote(schema)}.${quote(table)}`;
  const names = columns.map((c) => quote(c.name)).join(", ");
  const values = rows.map((row) => `  (${row.map(literal).join(", ")})`).join(",\n");
  return `INSERT INTO ${target} (${names}) VALUES\n${values};`;
}

/** The UPDATE statements that saving pending edits will run, one per changed row. */
export function pendingChangesToSql(
  schema: string,
  table: string,
  changes: { key: Record<string, unknown>; updates: Record<string, unknown> }[],
  dialect: SqlDialectName = "postgres",
): string {
  const quote = quoteIdentFor(dialect);
  const literal = sqlLiteralFor(dialect);
  const target = `${quote(schema)}.${quote(table)}`;
  return changes
    .map(({ key, updates }) => {
      const set = Object.entries(updates).map(([column, value]) => `${quote(column)} = ${literal(value)}`);
      const where = Object.entries(key).map(([column, value]) => `${quote(column)} = ${literal(value)}`);
      return `UPDATE ${target}\nSET ${set.join(",\n    ")}\nWHERE ${where.join(" AND ")};`;
    })
    .join("\n\n");
}
