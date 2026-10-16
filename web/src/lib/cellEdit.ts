import type { ColumnInfo } from "@/types";
// With the extension, so Node's test runner can load it too
import { dateModeOf, type DateMode } from "./dateValue.ts";


/** Text shown in the editor for a cell value (NULL starts as an empty field). */
export function toEditText(value: unknown): string {
  if (value === null || value === undefined) return "";
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

export type EditOutcome =
  | { changed: false }
  /** `value` is null for SQL NULL, otherwise the text to save. */
  | { changed: true; value: string | null };

/**
 * Decides what an edit means. NULL is its own state: an empty field is an empty
 * string, not NULL, unless the user explicitly set NULL.
 */
export function resolveEdit(original: unknown, isNull: boolean, text: string): EditOutcome {
  const originalIsNull = original === null || original === undefined;
  if (isNull) return originalIsNull ? { changed: false } : { changed: true, value: null };
  if (originalIsNull) return { changed: true, value: text };
  return text === toEditText(original) ? { changed: false } : { changed: true, value: text };
}

/** How a cell is edited, from its column's type and current value. */
export type CellEditor =
  | { kind: "readonly" }
  | { kind: "text" }
  /** A fixed set of values: booleans and enums. */
  | { kind: "choice"; options: string[] }
  /** A calendar and/or time field. */
  | { kind: "datetime"; mode: DateMode }
  /** Edited in a dialog: JSON, or text too long or multi-line for one line. */
  | { kind: "long"; json: boolean };

const LONG_TEXT = 120;

export function editorFor(column: ColumnInfo | undefined, value: unknown): CellEditor {
  if (column?.isGenerated) return { kind: "readonly" };
  if (column?.dataType === "boolean") return { kind: "choice", options: ["true", "false"] };
  if (column?.enumValues?.length) return { kind: "choice", options: column.enumValues };
  const dateMode = dateModeOf(column?.dataType);
  // A number in a date column (SQLite keeps dates as Unix times) is edited as the number it is
  if (dateMode && typeof value !== "number") return { kind: "datetime", mode: dateMode };
  if (column?.dataType === "json" || column?.dataType === "jsonb") return { kind: "long", json: true };
  if (typeof value === "string" && (value.length > LONG_TEXT || value.includes("\n"))) return { kind: "long", json: false };
  return { kind: "text" };
}

/** JSON shown indented in the editor; anything that doesn't parse is shown as is. */
export function toJsonEditText(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value !== "string") return JSON.stringify(value, null, 2);
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
