import { MariaSQL } from "@codemirror/lang-sql";
import catalog from "./catalog/mariadb.json" with { type: "json" };
import snippets from "./catalog/mariadb.snippets.json" with { type: "json" };
import { MYSQL_FAMILY_PHRASES } from "./mysql.ts";
import { quoter, wordsOf, wordsToQuote } from "./shared.ts";
import type { CatalogFunction, DialectSnippet, SqlDialect } from "./types.ts";

export const mariadb: SqlDialect = {
  engine: "mariadb",
  language: MariaSQL,
  quoteIdent: quoter(wordsToQuote(MariaSQL), "`", false),
  identifierQuote: "`",
  lineComments: ["--", "#"],
  ...wordsOf(MariaSQL),
  phrases: MYSQL_FAMILY_PHRASES,
  functions: catalog as CatalogFunction[],
  snippets: snippets as DialectSnippet[],
};
