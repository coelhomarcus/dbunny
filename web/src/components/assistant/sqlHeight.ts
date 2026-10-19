/** Room for a statement's lines, up to eight; the editor scrolls beyond that. */
export function sqlHeight(sql: string) {
  return Math.min(8, Math.max(1, sql.split("\n").length)) * 21 + 8;
}
