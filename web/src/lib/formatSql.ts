import type { SqlDialectName } from "./sqlStatements";

/**
 * Lays out SQL in the database's dialect: keywords upper-cased, clauses on their own lines.
 * The formatter is loaded the first time it's needed, so it stays out of the startup bundle.
 */
export async function formatSql(sql: string, dialect: SqlDialectName = "postgres"): Promise<string> {
  const { format } = await import("sql-formatter");
  const language = dialect === "sqlite" ? "sqlite" : dialect === "mysql" ? "mysql" : "postgresql";
  return format(sql, { language, keywordCase: "upper", tabWidth: 2, linesBetweenQueries: 1 });
}

const formattedCache = new Map<string, string>();
const CACHE_MAX = 128;

/** Same as {@link formatSql}, but reuses results for identical SQL text (e.g. activity monitor). */
export async function formatSqlCached(sql: string, dialect: SqlDialectName = "postgres"): Promise<string> {
  const key = `${dialect}\0${sql}`;
  const hit = formattedCache.get(key);
  if (hit) return hit;

  let out: string;
  try {
    out = await formatSql(sql, dialect);
  } catch {
    out = sql;
  }

  if (formattedCache.size >= CACHE_MAX) {
    const first = formattedCache.keys().next().value;
    if (first !== undefined) formattedCache.delete(first);
  }
  formattedCache.set(key, out);
  return out;
}

/** Clears the in-memory cache (tests). */
export function clearFormatSqlCache() {
  formattedCache.clear();
}
