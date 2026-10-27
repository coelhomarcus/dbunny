import { MySQL } from "@codemirror/lang-sql";
import catalog from "./catalog/mysql.json" with { type: "json" };
import { COMMON_PHRASES, quoter, wordsOf } from "./shared.ts";
import type { CatalogFunction, DialectItem, SqlDialect } from "./types.ts";

/** The phrases MySQL and MariaDB share. */
export const MYSQL_FAMILY_PHRASES: DialectItem[] = [
  ...COMMON_PHRASES,
  { label: "ON DUPLICATE KEY UPDATE", requires: "onDuplicateKeyUpdate" },
  { label: "INSERT IGNORE" },
  { label: "STRAIGHT_JOIN" },
  { label: "FOR UPDATE" },
  { label: "SKIP LOCKED" },
  { label: "WITH RECURSIVE", requires: "recursiveCte" },
  { label: "CREATE OR REPLACE" },
];

export const mysql: SqlDialect = {
  engine: "mysql",
  language: MySQL,
  quoteIdent: quoter(MySQL, "`", false),
  identifierQuote: "`",
  lineComments: ["--", "#"],
  ...wordsOf(MySQL),
  phrases: [...MYSQL_FAMILY_PHRASES, { label: "LEFT JOIN LATERAL", requires: "lateral" }],
  functions: catalog as CatalogFunction[],
  snippets: [],
};
