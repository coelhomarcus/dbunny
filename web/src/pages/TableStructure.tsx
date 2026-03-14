import { useEffect, useState } from "react";
import { useParams, NavLink } from "react-router";
import { Loader } from "lucide-react";
import { api } from "../lib/api";
import type { ColumnInfo } from "@/types";

export default function TableStructure() {
  const { schema, table } = useParams<{ schema: string; table: string }>();
  const [result, setResult] = useState<{ key: string; columns: ColumnInfo[] } | null>(null);
  const key = `${schema}.${table}`;

  useEffect(() => {
    if (!schema || !table) return;
    let stale = false;
    const k = `${schema}.${table}`;
    api.getColumns(schema, table).then((cols) => {
      if (!stale) setResult({ key: k, columns: cols });
    });
    return () => { stale = true; };
  }, [schema, table]);

  const loading = !result || result.key !== key;
  const columns = loading ? [] : result.columns;

  return (
    <div className="h-full flex flex-col gap-2">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-zinc-900 border border-zinc-800/60 rounded-xl shrink-0">
        <h2 className="text-base font-medium">
          <span className="text-zinc-500">{schema}.</span>
          {table}
        </h2>
        <div className="flex gap-1">
          <NavLink
            to={`/db/${schema}/${table}`}
            end
            className={({ isActive }) =>
              `px-3 py-1.5 text-sm rounded-lg transition-colors ${
                isActive
                  ? "bg-zinc-700 text-white"
                  : "text-zinc-400 hover:text-white"
              }`
            }
          >
            Data
          </NavLink>
          <NavLink
            to={`/db/${schema}/${table}/structure`}
            className={({ isActive }) =>
              `px-3 py-1.5 text-sm rounded-lg transition-colors ${
                isActive
                  ? "bg-zinc-700 text-white"
                  : "text-zinc-400 hover:text-white"
              }`
            }
          >
            Structure
          </NavLink>
        </div>
      </div>

      {loading ? (
        <div className="flex-1 flex items-end justify-end p-4">
          <Loader size={16} className="animate-spin text-zinc-500" />
        </div>
      ) : (
        <div className="flex-1 overflow-auto bg-zinc-900 border border-zinc-800/60 rounded-xl">
          <table className="w-full text-sm">
            <thead className="sticky top-0">
              <tr>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40">
                  #
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40">
                  Column
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40">
                  Type
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40">
                  Nullable
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-zinc-400 bg-zinc-800 border-b border-zinc-700/40">
                  Default
                </th>
              </tr>
            </thead>
            <tbody>
              {columns.map((col, i) => (
                <tr
                  key={col.name}
                  className={`border-b border-zinc-800/40 ${
                    i % 2 === 0 ? "bg-zinc-900" : "bg-zinc-800/20"
                  }`}
                >
                  <td className="px-4 py-2.5 text-zinc-600">
                    {col.ordinalPosition}
                  </td>
                  <td className="px-4 py-2.5 text-white font-medium">
                    {col.name}
                  </td>
                  <td className="px-4 py-2.5 text-amber-400">{col.dataType}</td>
                  <td className="px-4 py-2.5">
                    {col.isNullable ? (
                      <span className="text-zinc-500">YES</span>
                    ) : (
                      <span className="text-red-400">NO</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5 text-zinc-400 font-mono text-xs">
                    {col.defaultValue ?? (
                      <span className="text-zinc-600">-</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
