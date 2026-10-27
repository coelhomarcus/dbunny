import { SQLite } from "@codemirror/lang-sql";
import catalog from "./catalog/sqlite.json" with { type: "json" };
import { COMMON_PHRASES, quoter, wordsOf } from "./shared.ts";
import type { CatalogFunction, SqlDialect } from "./types.ts";

export const sqlite: SqlDialect = {
  engine: "sqlite",
  language: SQLite,
  quoteIdent: quoter(SQLite, '"', false),
  identifierQuote: '"',
  lineComments: ["--"],
  ...wordsOf(SQLite),
  phrases: [
    ...COMMON_PHRASES,
    { label: "FULL JOIN" },
    { label: "ON CONFLICT", requires: "onConflict" },
    { label: "DO NOTHING", requires: "onConflict" },
    { label: "DO UPDATE SET", requires: "onConflict" },
    { label: "NULLS FIRST", requires: "nullsOrdering" },
    { label: "NULLS LAST", requires: "nullsOrdering" },
    { label: "IS DISTINCT FROM" },
    { label: "IS NOT DISTINCT FROM" },
    { label: "INSERT OR REPLACE" },
    { label: "INSERT OR IGNORE" },
    { label: "WITH RECURSIVE", requires: "recursiveCte" },
    { label: "WITHOUT ROWID" },
  ],
  functions: catalog as CatalogFunction[],
  snippets: [],
};
