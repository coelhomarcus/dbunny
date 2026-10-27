import { PostgreSQL } from "@codemirror/lang-sql";
import catalog from "./catalog/postgres.json" with { type: "json" };
import { COMMON_PHRASES, quoter, wordsOf } from "./shared.ts";
import type { CatalogFunction, SqlDialect } from "./types.ts";

export const postgres: SqlDialect = {
  engine: "postgres",
  language: PostgreSQL,
  quoteIdent: quoter(PostgreSQL, '"', true),
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
