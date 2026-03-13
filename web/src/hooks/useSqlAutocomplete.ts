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

const PG_RESERVED = new Set(SQL_KEYWORDS.map((k) => k.split(" ")[0]));

interface DbTable {
  schema: string;
  name: string;
  columns?: ColumnInfo[];
}

/** Wrap identifier in double quotes when it needs quoting in PostgreSQL */
function quoteIdent(name: string): string {
  const needsQuoting =
    name !== name.toLowerCase() ||
    PG_RESERVED.has(name.toUpperCase()) ||
    /[^a-z0-9_]/.test(name) ||
    /^[0-9]/.test(name);
  return needsQuoting ? `"${name}"` : name;
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

        const lineContent = model.getValueInRange({
          startLineNumber: position.lineNumber,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });

        const textBeforeCursor = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });

        const suggestions: Array<{
          label: string;
          kind: number;
          insertText: string;
          range: typeof range;
          detail?: string;
          sortText?: string;
        }> = [];

        const tables = dbTablesRef.current;

        // ── Dot context: "schema." → tables in that schema, "table." → columns ──
        const dotMatch = lineContent.match(/(?:"([^"]+)"|(\w+))\.(\w*)$/);
        if (dotMatch) {
          const prefix = dotMatch[1] ?? dotMatch[2]; // the part before the dot
          const dotRange = {
            ...range,
            startColumn: position.column - (dotMatch[3]?.length ?? 0),
          };

          // Check if prefix is a schema name
          const schemaTablesMatch = tables.filter(
            (t) => t.schema.toLowerCase() === prefix.toLowerCase()
          );
          if (schemaTablesMatch.length > 0) {
            for (const t of schemaTablesMatch) {
              const quoted = quoteIdent(t.name);
              suggestions.push({
                label: t.name,
                kind: monaco.languages.CompletionItemKind.Class,
                insertText: quoted,
                detail: `${t.schema} - table`,
                range: dotRange,
                sortText: "0_" + t.name,
              });
            }
            return { suggestions };
          }

          // Check if prefix is a table name (or alias) → suggest columns
          const matchedTable = tables.find(
            (t) => t.name.toLowerCase() === prefix.toLowerCase()
          );
          if (matchedTable?.columns) {
            for (const col of matchedTable.columns) {
              const quoted = quoteIdent(col.name);
              suggestions.push({
                label: col.name,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: quoted,
                detail: `${col.dataType}${col.isPrimaryKey ? " (PK)" : ""}`,
                range: dotRange,
                sortText: "0_" + col.name,
              });
            }
            return { suggestions };
          }

          return { suggestions };
        }

        // ── Detect context ──
        const textUpper = textBeforeCursor.toUpperCase();
        const tableContextPattern = /(?:FROM|JOIN|INTO|UPDATE|TABLE)\s+\w*$/i;
        const isTableContext = tableContextPattern.test(textUpper);

        const columnSelectPattern =
          /(?:SELECT|WHERE|AND|OR|ON|SET|ORDER\s+BY|GROUP\s+BY|HAVING)\s+\w*$/i;
        const isColumnContext = columnSelectPattern.test(textUpper);

        // ── Keywords ──
        for (const kw of SQL_KEYWORDS) {
          suggestions.push({
            label: kw,
            kind: monaco.languages.CompletionItemKind.Keyword,
            insertText: kw,
            range,
            sortText: isTableContext ? "3_" + kw : "1_" + kw,
          });
        }

        // ── Tables ──
        for (const t of tables) {
          const quoted = quoteIdent(t.name);
          const sortPrefix = isTableContext ? "0_" : "2_";
          const label = t.schema === "public" ? t.name : `${t.schema}.${t.name}`;
          const insertText =
            t.schema === "public"
              ? quoted
              : `${quoteIdent(t.schema)}.${quoted}`;

          suggestions.push({
            label,
            kind: monaco.languages.CompletionItemKind.Class,
            insertText,
            detail: `${t.schema}.${t.name}`,
            range,
            sortText: sortPrefix + t.name,
          });
        }

        // ── Columns (deduplicated) ──
        if (isColumnContext) {
          // Find which tables are referenced in the query
          const referencedTableNames = new Set<string>();
          const tableRefPattern = /(?:FROM|JOIN)\s+(?:(?:"[^"]+"|[\w]+)\.)?(?:"([^"]+)"|(\w+))/gi;
          const fullText = model.getValue();
          let match;
          while ((match = tableRefPattern.exec(fullText)) !== null) {
            referencedTableNames.add((match[1] ?? match[2]).toLowerCase());
          }

          // Deduplicate: keep one entry per column name, prefer referenced tables
          const columnMap = new Map<
            string,
            { col: ColumnInfo; table: DbTable; isReferenced: boolean }
          >();

          for (const t of tables) {
            if (!t.columns) continue;
            const isReferenced = referencedTableNames.has(t.name.toLowerCase());
            for (const col of t.columns) {
              const key = col.name.toLowerCase();
              const existing = columnMap.get(key);
              // Prefer referenced table's column, or first seen
              if (!existing || (isReferenced && !existing.isReferenced)) {
                columnMap.set(key, { col, table: t, isReferenced });
              }
            }
          }

          for (const { col, table, isReferenced } of columnMap.values()) {
            const quoted = quoteIdent(col.name);
            suggestions.push({
              label: col.name,
              kind: monaco.languages.CompletionItemKind.Field,
              insertText: quoted,
              detail: `${table.name}.${col.name} (${col.dataType})`,
              range,
              sortText: isReferenced ? "0_" + col.name : "3_" + col.name,
            });
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
