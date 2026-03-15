import type { QueryColumn } from "@/types";

export type ExportFormat = "csv" | "json";

function escapeCsv(value: unknown): string {
  if (value === null || value === undefined) return "";
  const str = typeof value === "object" ? JSON.stringify(value) : String(value);
  if (str.includes(",") || str.includes('"') || str.includes("\n")) {
    return `"${str.replace(/"/g, '""')}"`;
  }
  return str;
}

function toCsv(columns: QueryColumn[], rows: unknown[][]): string {
  const header = columns.map((c) => escapeCsv(c.name)).join(",");
  const body = rows.map((row) => row.map(escapeCsv).join(","));
  return [header, ...body].join("\n");
}

function toJson(columns: QueryColumn[], rows: unknown[][]): string {
  const objects = rows.map((row) => {
    const obj: Record<string, unknown> = {};
    columns.forEach((col, i) => {
      obj[col.name] = row[i];
    });
    return obj;
  });
  return JSON.stringify(objects, null, 2);
}

export async function exportData(
  columns: QueryColumn[],
  rows: unknown[][],
  format: ExportFormat,
  defaultName: string,
) {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const { writeTextFile } = await import("@tauri-apps/plugin-fs");

  const content = format === "csv" ? toCsv(columns, rows) : toJson(columns, rows);

  const path = await save({
    defaultPath: `${defaultName}.${format}`,
    filters: [
      format === "csv"
        ? { name: "CSV", extensions: ["csv"] }
        : { name: "JSON", extensions: ["json"] },
    ],
  });

  if (path) await writeTextFile(path, content);
}
