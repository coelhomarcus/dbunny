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
