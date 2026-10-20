import { useMemo, useState } from "react";
import { Copy } from "lucide-react";
import AiIcon from "./ui/AiIcon";
import ContextMenu, { type ContextMenuItem } from "./ui/ContextMenu";
import { writeClipboardText } from "../lib/clipboard";
import { rowsToJson, rowsToTsv, valueText } from "../lib/copyRows";
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  type ColumnDef,
} from "@tanstack/react-table";
import type { QueryColumn, QueryResult } from "@/types";
import { QueryResultsSkeleton } from "./Skeleton";
import { isMac } from "../lib/platform";
import Button from "./ui/Button";

interface QueryResultsProps {
  result: QueryResult | null;
  error: string | null;
  running: boolean;
  /** Asks the assistant to fix the statement that failed; the button shows only when it is given. */
  onFix?: () => void;
}

type PgError = {
  message: string;
  severity?: string;
  detail?: string;
  hint?: string;
  position?: number;
  code?: string;
  /** Which statement of a script failed (1-based) and how many it had. */
  statement?: number | null;
  statementCount?: number | null;
};

function parseError(raw: string): PgError {
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed.message === "string") return parsed;
  } catch {}
  return { message: raw };
}

function ErrorDisplay({ raw, onFix }: { raw: string; onFix?: () => void }) {
  const err = parseError(raw);
  const severity = err.severity?.toUpperCase() ?? "ERROR";
  const severityColor =
    severity === "WARNING" ? "text-warning-foreground bg-warning/10 border-warning/30"
    : severity === "NOTICE" ? "text-info-foreground bg-info/10 border-info/30"
    : "text-destructive-foreground bg-destructive/10 border-destructive/30";

  return (
    <div className={`m-4 select-text rounded-lg border p-4 text-sm ${severityColor}`}>
      <div className="flex items-start gap-2.5">
        <div className="flex-1 min-w-0 space-y-2">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-semibold uppercase text-xs tracking-wider opacity-70">
              {severity}
            </span>
            {err.code && (
              <span className="font-mono text-xs opacity-50">{err.code}</span>
            )}
          </div>
          <p className="font-mono text-sm leading-relaxed">{err.message}</p>
          {err.statement != null && err.statementCount != null && (
            <p className="text-xs opacity-70">
              Statement {err.statement} of {err.statementCount}
              {err.statement > 1 && ". The statements before it already ran."}
            </p>
          )}
          {err.position != null && (
            <p className="text-xs opacity-70">
              Position: <span className="font-mono">{err.position}</span>
            </p>
          )}
          {err.detail && (
            <div className="pt-1 border-t border-current/20">
              <span className="text-xs font-medium opacity-60 uppercase tracking-wider">Detail</span>
              <p className="font-mono text-xs mt-0.5 opacity-80">{err.detail}</p>
            </div>
          )}
          {err.hint && (
            <div className="pt-1 border-t border-current/20">
              <span className="text-xs font-medium opacity-60 uppercase tracking-wider">Hint</span>
              <p className="font-mono text-xs mt-0.5 opacity-80">{err.hint}</p>
            </div>
          )}
          {onFix && (
            <div className="pt-1">
              <Button variant="outline" size="xs" onClick={onFix}>
                <AiIcon />
                Fix with AI
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default function QueryResults({ result, error, running, onFix }: QueryResultsProps) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const columns = useMemo<ColumnDef<unknown[], unknown>[]>(() => {
    if (!result) return [];
    return result.columns.map((col: QueryColumn, index: number) => {
      const displayName = col.name === "?column?" ? `column_${index + 1}` : col.name;
      return {
      id: `${col.name}_${index}`,
      header: displayName,
      accessorFn: (row: unknown[]) => row[index],
      cell: (info) => {
        const value = info.getValue();
        if (value === null) return <span className="text-muted-foreground/70 italic">NULL</span>;
        if (typeof value === "object") return JSON.stringify(value);
        return String(value);
      },
      };
    });
  }, [result]);

  const table = useReactTable({
    data: result?.rows ?? [],
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    <div className="flex-1 min-h-0 overflow-auto flex flex-col h-full">
      {error && <ErrorDisplay raw={error} onFix={onFix} />}

      {result && result.columns.length > 0 && (
        <table
          className="w-full text-xs"
          style={{ minWidth: result.columns.length * 150 }}
        >
          <thead className="sticky top-0 z-10 select-none">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((header) => (
                  <th
                    key={header.id}
                    className="whitespace-nowrap border-b border-border bg-card-header px-3 py-1.5 text-left text-xs font-medium text-muted-foreground"
                  >
                    {flexRender(header.column.columnDef.header, header.getContext())}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row, i) => (
              <tr
                key={row.id}
                className={`border-b border-border hover:bg-accent ${
                  i % 2 === 0 ? "bg-card" : "bg-muted"
                }`}
              >
                {row.getVisibleCells().map((cell) => (
                  <td
                    key={cell.id}
                    className="max-w-xs truncate whitespace-nowrap px-3 py-1 font-mono text-xs text-foreground"
                    onContextMenu={(e) => {
                      if (!result) return;
                      e.preventDefault();
                      const value = cell.getValue();
                      const values = row.original as unknown[];
                      setMenu({
                        x: e.clientX,
                        y: e.clientY,
                        items: [
                          { label: "Copy value", icon: <Copy />, onSelect: () => writeClipboardText(valueText(value)) },
                          "separator",
                          { label: "Copy row as JSON", onSelect: () => writeClipboardText(rowsToJson(result.columns, [values])) },
                          { label: "Copy row as TSV", onSelect: () => writeClipboardText(rowsToTsv(result.columns, [values])) },
                        ],
                      });
                    }}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {result && result.columns.length === 0 && (
        <div className="flex-1 flex items-center justify-center p-4 text-muted-foreground text-base">
          Query executed successfully. {result.rowsAffected ?? result.rowCount} rows affected.
        </div>
      )}

      <ContextMenu position={menu} items={menu?.items ?? []} onClose={() => setMenu(null)} />

      {running && !result && <QueryResultsSkeleton />}

      {!result && !error && !running && (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground select-none">
          <span className="flex items-center gap-2">
            <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-sans text-xs">{isMac ? "⌘ ↵" : "Ctrl ↵"}</kbd>
            runs the statement at the cursor
          </span>
          <span className="flex items-center gap-2">
            <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-sans text-xs">{isMac ? "⇧ ⌘ ↵" : "Shift Ctrl ↵"}</kbd>
            runs everything
          </span>
        </div>
      )}
    </div>
  );
}
