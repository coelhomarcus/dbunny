import { useMemo } from "react";
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  type ColumnDef,
} from "@tanstack/react-table";
import type { QueryColumn, QueryResult } from "@/types";
import { QueryResultsSkeleton } from "./Skeleton";

interface QueryResultsProps {
  result: QueryResult | null;
  error: string | null;
  running: boolean;
}

type PgError = {
  message: string;
  severity?: string;
  detail?: string;
  hint?: string;
  position?: number;
  code?: string;
};

function parseError(raw: string): PgError {
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed.message === "string") return parsed;
  } catch {}
  return { message: raw };
}

function ErrorDisplay({ raw }: { raw: string }) {
  const err = parseError(raw);
  const severity = err.severity?.toUpperCase() ?? "ERROR";
  const severityColor =
    severity === "WARNING" ? "text-yellow-400 bg-yellow-950/40 border-yellow-900/50"
    : severity === "NOTICE" ? "text-blue-400 bg-blue-950/40 border-blue-900/50"
    : "text-red-400 bg-red-950/40 border-red-900/50";

  return (
    <div className={`m-4 rounded-lg border p-4 text-sm ${severityColor}`}>
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
        </div>
      </div>
    </div>
  );
}

export default function QueryResults({ result, error, running }: QueryResultsProps) {
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
        if (value === null) return <span className="text-zinc-600 italic">NULL</span>;
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
      {error && <ErrorDisplay raw={error} />}

      {result && result.columns.length > 0 && (
        <table
          className="w-full text-sm"
          style={{ minWidth: result.columns.length * 150 }}
        >
          <thead className="sticky top-0 z-10 select-none">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((header) => (
                  <th
                    key={header.id}
                    className="text-left px-3 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40 whitespace-nowrap"
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
                className={`border-b border-zinc-800/40 hover:bg-zinc-800/40 ${
                  i % 2 === 0 ? "bg-zinc-900" : "bg-zinc-800/20"
                }`}
              >
                {row.getVisibleCells().map((cell) => (
                  <td
                    key={cell.id}
                    className="px-3 py-2 text-zinc-300 whitespace-nowrap max-w-xs truncate font-mono text-sm"
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
        <div className="flex-1 flex items-center justify-center p-4 text-zinc-400 text-base">
          Query executed successfully. {result.rowsAffected ?? result.rowCount} rows affected.
        </div>
      )}

      {running && !result && <QueryResultsSkeleton />}

      {!result && !error && !running && (
        <div className="flex-1 flex items-center justify-center text-zinc-600 text-base">
          Write a query and press Ctrl+Enter to run
        </div>
      )}
    </div>
  );
}
