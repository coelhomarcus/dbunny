import type { ReactNode } from "react";
import { FileUp, Loader, RefreshCw, MoveHorizontal, TableProperties } from "lucide-react";
import type { QueryColumn } from "@/types";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import ExportMenu from "../ExportMenu";

/** Row count and the table's secondary tools, in the bar under the rows. */
export default function TableTools({
  schema,
  table,
  totalRows,
  totalRowsEstimated,
  counting,
  onCountExactly,
  refreshing,
  loading,
  spinKey,
  onRefresh,
  onOpenStructure,
  columnsMenu,
  readOnly,
  onImportCsv,
  hasCustomWidths,
  onResetWidths,
  columns,
  rows,
  onFetchAllRows,
}: {
  schema: string;
  table: string;
  totalRows?: number;
  /** totalRows comes from the statistics; clicking it counts exactly. */
  totalRowsEstimated?: boolean;
  counting: boolean;
  onCountExactly: () => void;
  refreshing: boolean;
  loading: boolean;
  spinKey: number;
  onRefresh: () => void;
  onOpenStructure: () => void;
  columnsMenu: ReactNode;
  readOnly: boolean;
  onImportCsv: () => void;
  hasCustomWidths: boolean;
  onResetWidths: () => void;
  columns?: QueryColumn[];
  rows?: unknown[][];
  onFetchAllRows?: () => Promise<{ columns: QueryColumn[]; rows: unknown[][] }>;
}) {
  const label = totalRows === undefined ? "" : `${totalRows.toLocaleString()} row${totalRows === 1 ? "" : "s"}`;
  return (
    <>
      {totalRows !== undefined &&
        (totalRowsEstimated ? (
          <Tooltip label="An estimate. Click for the exact count" side="top">
            <button
              type="button"
              onClick={onCountExactly}
              disabled={counting}
              className="flex h-6 items-center gap-1 rounded-md px-1.5 tabular-nums text-foreground transition-colors cursor-pointer hover:bg-accent disabled:cursor-default"
            >
              {counting ? <Loader size={11} className="animate-spin" /> : "~"}
              {label}
            </button>
          </Tooltip>
        ) : (
          <span className="px-1.5 tabular-nums text-foreground">{label}</span>
        ))}
      <span className="mx-1 h-4 w-px bg-border" />
      <Tooltip label="Refresh" side="top">
        <Button variant="ghost-muted" size="icon-xs" onClick={onRefresh} disabled={refreshing || loading} aria-label="Refresh">
          <RefreshCw
            key={spinKey}
            className={refreshing ? "animate-spin" : spinKey > 0 ? "animate-[spin_0.5s_ease-in-out]" : ""}
          />
        </Button>
      </Tooltip>
      <Tooltip label="Structure" side="top">
        <Button variant="ghost-muted" size="icon-xs" onClick={onOpenStructure} aria-label="Structure">
          <TableProperties />
        </Button>
      </Tooltip>
      {columnsMenu}
      {hasCustomWidths && (
        <Tooltip label="Reset column widths" side="top">
          <Button variant="ghost-muted" size="icon-xs" onClick={onResetWidths} aria-label="Reset column widths">
            <MoveHorizontal />
          </Button>
        </Tooltip>
      )}
      {!readOnly && (
        <Tooltip label="Import a CSV file" side="top">
          <Button variant="ghost-muted" size="icon-xs" onClick={onImportCsv} aria-label="Import CSV">
            <FileUp />
          </Button>
        </Tooltip>
      )}
      {columns && rows && columns.length > 0 && (
        <ExportMenu
          columns={columns}
          rows={rows}
          defaultName={`${schema}.${table}`}
          totalRows={totalRows}
          totalRowsEstimated={totalRowsEstimated}
          onFetchAll={onFetchAllRows}
          side="up"
          compact
        />
      )}
    </>
  );
}
