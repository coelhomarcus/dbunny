import { useCallback, useRef, useEffect } from "react";
import type { BeforeMount } from "@monaco-editor/react";
import type { editor as monacoEditor, IPosition } from "monaco-editor";
import { api } from "../lib/api";
import type { ColumnInfo } from "@/types";

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

interface DbTable {
  schema: string;
  name: string;
  columns?: ColumnInfo[];
}

export function useSqlAutocomplete() {
  const dbTablesRef = useRef<DbTable[]>([]);
  const completionDisposableRef = useRef<{ dispose(): void } | null>(null);

  const loadDbSchema = useCallback(async () => {
    try {
      const schemas = await api.getSchemas();
      const tables: DbTable[] = [];
      for (const schema of schemas) {
        const [schemaTables, schemaViews] = await Promise.all([
          api.getTables(schema.name).catch(() => []),
          api.getViews(schema.name).catch(() => []),
        ]);
        for (const t of [...schemaTables, ...schemaViews]) {
          tables.push({ schema: t.schema, name: t.name });
        }
      }
      await Promise.all(
        tables.map(async (t) => {
          try {
            t.columns = await api.getColumns(t.schema, t.name);
          } catch { /* ignore */ }
        })
      );
      dbTablesRef.current = tables;
    } catch { /* not connected yet */ }
  }, []);

  const registerCompletionProvider = useCallback((monaco: Parameters<BeforeMount>[0]) => {
    if (completionDisposableRef.current) {
      completionDisposableRef.current.dispose();
    }

    completionDisposableRef.current = monaco.languages.registerCompletionItemProvider("sql", {
      triggerCharacters: [" ", ".", "\n"],
      provideCompletionItems(model: monacoEditor.ITextModel, position: IPosition) {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        const textBeforeCursor = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        }).toUpperCase();

        const suggestions: Array<{
          label: string;
          kind: number;
          insertText: string;
          range: typeof range;
          detail?: string;
          sortText?: string;
        }> = [];

        const tableContextPattern = /(?:FROM|JOIN|INTO|UPDATE|TABLE)\s+\w*$/i;
        const isTableContext = tableContextPattern.test(textBeforeCursor);

        const afterTablePattern = /(?:FROM|JOIN)\s+(?:(\w+)\.)?(\w+)\s+(?:WHERE|ON|SET|AND|OR|SELECT)?\s*\w*$/i;
        const columnSelectPattern = /(?:SELECT|WHERE|AND|OR|ON|SET|ORDER\s+BY|GROUP\s+BY|HAVING)\s+\w*$/i;
        const afterTableMatch = textBeforeCursor.match(afterTablePattern);
        const isColumnContext = columnSelectPattern.test(textBeforeCursor) || !!afterTableMatch;

        for (const kw of SQL_KEYWORDS) {
          suggestions.push({
            label: kw,
            kind: monaco.languages.CompletionItemKind.Keyword,
            insertText: kw,
            range,
            sortText: "1_" + kw,
          });
        }

        const tables = dbTablesRef.current;

        for (const t of tables) {
          const sortPrefix = isTableContext ? "0_" : "2_";
          suggestions.push({
            label: t.name,
            kind: monaco.languages.CompletionItemKind.Class,
            insertText: t.name,
            detail: `${t.schema}.${t.name}`,
            range,
            sortText: sortPrefix + t.name,
          });
          if (t.schema !== "public") {
            suggestions.push({
              label: `${t.schema}.${t.name}`,
              kind: monaco.languages.CompletionItemKind.Class,
              insertText: `${t.schema}.${t.name}`,
              detail: "table",
              range,
              sortText: sortPrefix + `${t.schema}.${t.name}`,
            });
          }
        }

        if (isColumnContext) {
          const referencedTableNames = new Set<string>();
          const tableRefPattern = /(?:FROM|JOIN)\s+(?:\w+\.)?(\w+)/gi;
          const fullText = model.getValue();
          let match;
          while ((match = tableRefPattern.exec(fullText)) !== null) {
            referencedTableNames.add(match[1].toLowerCase());
          }

          for (const t of tables) {
            if (!t.columns) continue;
            const isReferenced = referencedTableNames.has(t.name.toLowerCase());
            for (const col of t.columns) {
              suggestions.push({
                label: col.name,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: col.name,
                detail: `${t.name}.${col.name} (${col.dataType})`,
                range,
                sortText: isReferenced ? "0_" + col.name : "3_" + col.name,
              });
            }
          }
        }

        return { suggestions };
      },
    });
  }, []);

  useEffect(() => {
    loadDbSchema();
  }, [loadDbSchema]);

  return { registerCompletionProvider };
}
