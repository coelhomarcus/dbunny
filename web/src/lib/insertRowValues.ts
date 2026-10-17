import type { ColumnInfo } from "@/types";

export type InsertField = {
  /** The user typed in it (even to clear it): an untouched field is left to the default. */
  touched: boolean;
  isNull: boolean;
  text: string;
};

export const UNTOUCHED_INSERT_FIELD: InsertField = { touched: false, isNull: false, text: "" };

/** Values to insert: null is SQL NULL, and columns left alone are omitted. */
export function collectInsertValues(columns: ColumnInfo[], fields: Record<string, InsertField>) {
  const values: Record<string, string | null> = {};
  for (const column of columns) {
    const field = fields[column.name] ?? UNTOUCHED_INSERT_FIELD;
    if (field.isNull) values[column.name] = null;
    else if (field.touched) values[column.name] = field.text;
  }
  return values;
}
