/**
 * Lays out SQL in the PostgreSQL dialect: keywords upper-cased, clauses on their own lines.
 * The formatter is loaded the first time it's needed, so it stays out of the startup bundle.
 */
export async function formatSql(sql: string): Promise<string> {
  const { format } = await import("sql-formatter");
  return format(sql, { language: "postgresql", keywordCase: "upper", tabWidth: 2, linesBetweenQueries: 1 });
}
