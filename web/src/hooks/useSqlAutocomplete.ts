import { useCallback, useRef } from "react";
import type { BeforeMount } from "@monaco-editor/react";
import type { editor as monacoEditor, IPosition } from "monaco-editor";

const SQL_KEYWORDS = [
  "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET",
  "DELETE", "CREATE", "ALTER", "DROP", "TABLE", "INDEX", "VIEW",
  "INNER JOIN", "LEFT JOIN", "RIGHT JOIN", "FULL JOIN", "CROSS JOIN",
  "JOIN", "ON", "AND", "OR", "NOT", "IN", "EXISTS", "BETWEEN", "LIKE",
  "IS NULL", "IS NOT NULL", "ORDER BY", "GROUP BY", "HAVING", "LIMIT",
  "OFFSET", "AS", "DISTINCT", "COUNT", "SUM", "AVG", "MIN", "MAX",
  "CASE", "WHEN", "THEN", "ELSE", "END", "UNION", "UNION ALL",
  "EXCEPT", "INTERSECT", "WITH", "RETURNING", "CASCADE", "CONSTRAINT",
  "PRIMARY KEY", "FOREIGN KEY", "REFERENCES", "DEFAULT", "NOT NULL",
  "UNIQUE", "CHECK", "SERIAL", "BIGSERIAL", "TEXT", "INTEGER", "BIGINT",
  "BOOLEAN", "TIMESTAMP", "DATE", "NUMERIC", "VARCHAR", "CHAR", "JSON",
  "JSONB", "UUID", "ARRAY", "COALESCE", "NULLIF", "CAST",
  "ASC", "DESC", "NULLS FIRST", "NULLS LAST", "FETCH", "NEXT", "ROWS",
  "ONLY", "LATERAL", "TRUNCATE", "EXPLAIN", "ANALYZE", "BEGIN",
  "COMMIT", "ROLLBACK", "GRANT", "REVOKE",
];

export function useSqlAutocomplete() {
  const completionDisposableRef = useRef<{ dispose(): void } | null>(null);

  const registerCompletionProvider = useCallback((monaco: Parameters<BeforeMount>[0]) => {
    if (completionDisposableRef.current) {
      completionDisposableRef.current.dispose();
    }

    completionDisposableRef.current = monaco.languages.registerCompletionItemProvider("sql", {
      triggerCharacters: [" ", "\n"],
      provideCompletionItems(model: monacoEditor.ITextModel, position: IPosition) {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        const suggestions = SQL_KEYWORDS.map((kw) => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: kw,
          range,
        }));

        return { suggestions };
      },
    });
  }, []);

  return { registerCompletionProvider };
}
