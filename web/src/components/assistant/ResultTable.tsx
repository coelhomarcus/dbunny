import type { ReactNode } from "react";

/**
 * Rows in a table that scrolls sideways (and down, past a few rows) instead of squeezing its
 * columns, with the header kept in view. Every table in the conversation looks like this: the rows
 * a query gave and the tables the assistant writes in its answer.
 */
export default function ResultTable({ columns, rows }: { columns: ReactNode[]; rows: ReactNode[][] }) {
  return (
    <div className="max-h-56 overflow-auto">
      <table className="w-full border-separate border-spacing-0 text-left text-xs">
        <thead className="sticky top-0 z-10">
          <tr>
            {columns.map((column, index) => (
              <th key={index} className="whitespace-nowrap border-b border-border bg-card-header px-3 py-1.5 font-medium">
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="[&>tr:last-child>td]:border-b-0">
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.map((cell, cellIndex) => (
                <td key={cellIndex} className="max-w-md truncate whitespace-nowrap border-b border-border px-3 py-1 font-mono">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
