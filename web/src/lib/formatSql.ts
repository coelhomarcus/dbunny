import type { SqlDialectName } from "./sqlStatements";

/**
 * Lays out SQL in the database's dialect: keywords upper-cased, clauses on their own lines.
 * The formatter is loaded the first time it's needed, so it stays out of the startup bundle.
 */
export async function formatSql(sql: string, dialect: SqlDialectName = "postgres"): Promise<string> {
  const { format } = await import("sql-formatter");
  return format(sql, { language: dialect === "sqlite" ? "sqlite" : "postgresql", keywordCase: "upper", tabWidth: 2, linesBetweenQueries: 1 });
}
