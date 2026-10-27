import type { SQLDialect } from "@codemirror/lang-sql";
import type { DialectItem } from "./types.ts";

/** Phrases every dialect understands; the dialect only completes single words. */
export const COMMON_PHRASES: DialectItem[] = [
  "INNER JOIN", "LEFT JOIN", "LEFT OUTER JOIN", "RIGHT JOIN", "CROSS JOIN",
  "IS NULL", "IS NOT NULL", "NOT NULL", "ORDER BY", "GROUP BY", "PARTITION BY",
  "UNION ALL", "PRIMARY KEY", "FOREIGN KEY", "INSERT INTO", "DELETE FROM",
  "IF EXISTS", "IF NOT EXISTS",
].map((label) => ({ label }));

const words = (list: string | undefined) => (list ?? "").split(/\s+/).filter(Boolean);

/** The keywords and types lang-sql lists for a dialect, upper case. */
export function wordsOf(language: SQLDialect) {
  return {
    keywords: words(language.spec.keywords).map((w) => w.toUpperCase()),
    types: words(language.spec.types).map((w) => w.toUpperCase()),
  };
}

/**
 * Quotes a name only when it has to be: a reserved word, a character outside a plain name, or
 * (`foldsCase`, PostgreSQL) an upper-case letter the server would fold away.
 */
export function quoter(language: SQLDialect, quote: '"' | "`", foldsCase: boolean) {
  const spec = language.spec;
  const reserved = new Set(words([spec.keywords, spec.types, spec.builtin].join(" ")).map((w) => w.toLowerCase()));
  const plain = foldsCase ? /^[a-z_][a-z0-9_$]*$/ : /^[A-Za-z_][A-Za-z0-9_$]*$/;
  return (name: string) => {
    if (plain.test(name) && !reserved.has(name.toLowerCase())) return name;
    return `${quote}${name.split(quote).join(quote + quote)}${quote}`;
  };
}
