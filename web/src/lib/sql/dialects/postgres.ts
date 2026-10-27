import { PostgreSQL } from "@codemirror/lang-sql";
import catalog from "./catalog/postgres.json" with { type: "json" };
import { COMMON_PHRASES, quoter, wordsOf } from "./shared.ts";
import type { CatalogFunction, SqlDialect } from "./types.ts";

/**
 * The words PostgreSQL reserves, or reserves except as a function or type name (Appendix C); its
 * other keywords are fine as column and table names. lang-sql's list also holds those, and
 * function names, so it would quote `id` and `name`.
 */
const RESERVED = `all analyse analyze and any array as asc asymmetric authorization binary both case cast check
  collate collation column concurrently constraint create cross current_catalog current_date current_role
  current_schema current_time current_timestamp current_user default deferrable desc distinct do else end
  except false fetch for foreign freeze from full grant group having ilike in initially inner intersect into
  is isnull join lateral leading left like limit localtime localtimestamp natural not notnull null offset on
  only or order outer overlaps placing primary references returning right select session_user similar some
  symmetric system_user table tablesample then to trailing true union unique user using variadic verbose
  when where window with`.split(/\s+/);

export const postgres: SqlDialect = {
  engine: "postgres",
  language: PostgreSQL,
  quoteIdent: quoter(RESERVED, '"', true),
  identifierQuote: '"',
  lineComments: ["--"],
  ...wordsOf(PostgreSQL),
  phrases: [
    ...COMMON_PHRASES,
    { label: "FULL JOIN" },
    { label: "FULL OUTER JOIN" },
    { label: "LEFT JOIN LATERAL", requires: "lateral" },
    { label: "CROSS JOIN LATERAL", requires: "lateral" },
    { label: "ON CONFLICT", requires: "onConflict" },
    { label: "DO NOTHING", requires: "onConflict" },
    { label: "DO UPDATE SET", requires: "onConflict" },
    { label: "NULLS FIRST", requires: "nullsOrdering" },
    { label: "NULLS LAST", requires: "nullsOrdering" },
    { label: "NULLS NOT DISTINCT", requires: "nullsNotDistinct" },
    { label: "IS DISTINCT FROM" },
    { label: "IS NOT DISTINCT FROM" },
    { label: "DISTINCT ON" },
    { label: "FOR UPDATE" },
    { label: "SKIP LOCKED" },
    { label: "WITH RECURSIVE", requires: "recursiveCte" },
    { label: "CREATE OR REPLACE" },
    { label: "MERGE INTO", requires: "merge" },
  ],
  functions: catalog as CatalogFunction[],
  snippets: [],
};
