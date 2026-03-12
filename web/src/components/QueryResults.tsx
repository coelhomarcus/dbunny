import { useMemo } from "react";
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  type ColumnDef,
} from "@tanstack/react-table";
import type { QueryColumn, QueryResult } from "@/types";

interface QueryResultsProps {
  result: QueryResult | null;
  error: string | null;
  running: boolean;
}

export default function QueryResults({ result, error, running }: QueryResultsProps) {
  const columns = useMemo<ColumnDef<unknown[], unknown>[]>(() => {
    if (!result) return [];
    return result.columns.map((col: QueryColumn, index: number) => ({
      id: col.name,
      header: col.name,
      accessorFn: (row: unknown[]) => row[index],
      cell: (info) => {
        const value = info.getValue();
        if (value === null) return <span className="text-zinc-600 italic">NULL</span>;
        if (typeof value === "object") return JSON.stringify(value);
        return String(value);
      },
    }));
  }, [result]);

  const table = useReactTable({
    data: result?.rows ?? [],
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    <div className="flex-1 min-h-0 overflow-auto">
      {error && (
        <div className="p-4 text-red-400 text-sm font-mono whitespace-pre-wrap">{error}</div>
      )}

      {result && result.columns.length > 0 && (
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-zinc-900 z-10">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((header) => (
                  <th
                    key={header.id}
                    className="text-left px-3 py-2 text-xs font-medium text-zinc-400 border-b border-zinc-800 whitespace-nowrap"
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
                className={`border-b border-zinc-800/50 hover:bg-zinc-800/50 ${
                  i % 2 === 0 ? "bg-zinc-950" : "bg-zinc-900/30"
                }`}
              >
                {row.getVisibleCells().map((cell) => (
                  <td
                    key={cell.id}
                    className="px-3 py-1.5 text-zinc-300 whitespace-nowrap max-w-xs truncate font-mono text-xs"
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
        <div className="p-4 text-zinc-400 text-sm">
          Query executed successfully. {result.rowCount} rows affected.
        </div>
      )}

      {!result && !error && !running && (
        <div className="flex-1 flex items-center justify-center h-full text-zinc-600 text-sm">
          Write a query and press Ctrl+Enter to run
        </div>
      )}
    </div>
  );
}
