import type { ReactNode } from "react";
import { FileUp, RefreshCw, MoveHorizontal, TableProperties } from "lucide-react";
import Spinner from "../ui/Spinner";
import type { QueryColumn } from "@/types";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import ExportMenu from "../ExportMenu";
import { useT } from "../../i18n";

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
  const t = useT();
  const label = totalRows === undefined ? "" : t("table.tableTools.rows", { count: totalRows });
  return (
    <>
      {totalRows !== undefined &&
        (totalRowsEstimated ? (
          <Tooltip label={t("table.tableTools.anEstimateClickForThe")} side="top">
            <button
              type="button"
              onClick={onCountExactly}
              disabled={counting}
              className="flex h-6 items-center gap-1 rounded-md px-1.5 tabular-nums text-foreground transition-colors cursor-pointer hover:bg-accent disabled:cursor-default"
            >
              {counting ? <Spinner size={11} /> : "~"}
              {label}
            </button>
          </Tooltip>
        ) : (
          <span className="px-1.5 tabular-nums text-foreground">{label}</span>
        ))}
      <span className="mx-1 h-4 w-px bg-border" />
      <Tooltip label={t("table.tableTools.refresh")} side="top">
        <Button variant="ghost-muted" size="icon-xs" onClick={onRefresh} disabled={refreshing || loading} aria-label={t("table.tableTools.refresh")}>
          <RefreshCw
            key={spinKey}
            className={refreshing ? "animate-spin" : spinKey > 0 ? "animate-[spin_0.5s_ease-in-out]" : ""}
          />
        </Button>
      </Tooltip>
      <Tooltip label={t("table.tableTools.structure")} side="top">
        <Button variant="ghost-muted" size="icon-xs" onClick={onOpenStructure} aria-label={t("table.tableTools.structure")}>
          <TableProperties />
        </Button>
      </Tooltip>
      {columnsMenu}
      {hasCustomWidths && (
        <Tooltip label={t("table.tableTools.resetColumnWidths")} side="top">
          <Button variant="ghost-muted" size="icon-xs" onClick={onResetWidths} aria-label={t("table.tableTools.resetColumnWidths")}>
            <MoveHorizontal />
          </Button>
        </Tooltip>
      )}
      {!readOnly && (
        <Tooltip label={t("table.tableTools.importACsvFile")} side="top">
          <Button variant="ghost-muted" size="icon-xs" onClick={onImportCsv} aria-label={t("table.tableTools.importCsv")}>
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
