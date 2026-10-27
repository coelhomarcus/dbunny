import type { SQLDialect } from "@codemirror/lang-sql";
import type { Engine, SqlFeature } from "@/types";

export type FunctionCategory = "text" | "numeric" | "datetime" | "json" | "aggregate" | "window" | "conditional" | "system";

/** A word or phrase the editor offers, with the server feature it needs. */
export interface DialectItem {
  label: string;
  requires?: SqlFeature;
}

/** A built-in function the dialect documents; its description is `sqlFunctions.<engine>.<key>`. */
export interface CatalogFunction {
  name: string;
  signature: string;
  category: FunctionCategory;
  requires?: SqlFeature;
  /** A statement that runs on every version the function is offered for. */
  example: string;
}

/** A statement template: `label` is the word that offers it, `sqlSnippets.<id>` says what it does. */
export interface DialectSnippet {
  id: string;
  label: string;
  /** CodeMirror snippet text, with `${field}` placeholders. */
  template: string;
  requires?: SqlFeature;
  example: string;
}

/** Everything the editor knows about one database's SQL. */
export interface SqlDialect {
  engine: Engine;
  language: SQLDialect;
  quoteIdent(name: string): string;
  identifierQuote: '"' | "`";
  lineComments: string[];
  keywords: string[];
  types: string[];
  phrases: DialectItem[];
  functions: CatalogFunction[];
  snippets: DialectSnippet[];
}
