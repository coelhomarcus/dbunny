import { useEffect, useState } from "react";
import { segmentedItemClass, segmentedListClass } from "../components/ui/styles";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { useParams, NavLink } from "react-router";
import { TableSkeleton } from "../components/Skeleton";
import { useApi } from "../contexts/useConnection";
import type { ColumnInfo } from "@/types";

export default function TableStructure() {
  const { schema, table } = useParams<{ schema: string; table: string }>();
  const api = useApi();
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
  }, [schema, table, api]);

  const loading = !result || result.key !== key;
  const columns = loading ? [] : result.columns;

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <PageHeader className="justify-between">
        <HeaderTitle label={`Structure · ${schema}`}>
          {table}
        </HeaderTitle>
        <div className={segmentedListClass}>
          <NavLink to={`/db/${schema}/${table}`} end className={({ isActive }) => segmentedItemClass(isActive)}>
            Data
          </NavLink>
          <NavLink to={`/db/${schema}/${table}/structure`} className={({ isActive }) => segmentedItemClass(isActive)}>
            Structure
          </NavLink>
        </div>
      </PageHeader>

      {loading ? (
        <TableSkeleton columns={5} rows={10} />
      ) : (
        <div className="flex-1 overflow-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0">
              <tr>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-muted-foreground bg-card-header border-b border-border">
                  #
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-muted-foreground bg-card-header border-b border-border">
                  Column
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-muted-foreground bg-card-header border-b border-border">
                  Type
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-muted-foreground bg-card-header border-b border-border">
                  Nullable
                </th>
                <th className="text-left px-4 py-2.5 text-sm font-medium text-muted-foreground bg-card-header border-b border-border">
                  Default
                </th>
              </tr>
            </thead>
            <tbody>
              {columns.map((col, i) => (
                <tr
                  key={col.name}
                  className={`border-b border-border ${
                    i % 2 === 0 ? "bg-card" : "bg-muted"
                  }`}
                >
                  <td className="px-4 py-2.5 text-muted-foreground/70">
                    {col.ordinalPosition}
                  </td>
                  <td className="px-4 py-2.5 text-foreground font-medium">
                    {col.name}
                  </td>
                  <td className="px-4 py-2.5 text-warning-foreground">{col.dataType}</td>
                  <td className="px-4 py-2.5">
                    {col.isNullable ? (
                      <span className="text-muted-foreground">YES</span>
                    ) : (
                      <span className="text-destructive-foreground">NO</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5 text-muted-foreground font-mono text-xs">
                    {col.defaultValue ?? (
                      <span className="text-muted-foreground/70">-</span>
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
