import { NavLink } from "react-router";
import {
  Save,
  X,
  Trash2,
  RefreshCw,
  Columns3,
  ListFilter,
  Lock,
  FileCode,
  Loader,
  Plus,
} from "lucide-react";
import ExportMenu from "./ExportMenu";
import Button from "./ui/Button";
import Badge from "./ui/Badge";
import { HeaderTitle, PageHeader } from "./layout/PageHeader";
import { segmentedItemClass, segmentedListClass } from "./ui/styles";
import type { QueryColumn } from "@/types";

interface TableViewHeaderProps {
  schema: string;
  table: string;
  totalRows?: number;
  totalRowsEstimated?: boolean;
  counting: boolean;
  onCountExactly: () => void;
  hasNoPk: boolean;
  /** Read-only connection: no insert, edit or delete. */
  readOnly: boolean;
  selectedCount: number;
  deleting: boolean;
  onDelete: () => void;
  hasPendingChanges: boolean;
  dirtyRowCount: number;
  saveError: string | null;
  saving: boolean;
  onSave: () => void;
  onDiscard: () => void;
  /** Shows the SQL that saving would run. */
  onReviewSql: () => void;
  refreshing: boolean;
  loading: boolean;
  spinKey: number;
  onRefresh: () => void;
  hasCustomWidths: boolean;
  onResetWidths: () => void;
  onInsertRow: () => void;
  filterCount: number;
  filtersOpen: boolean;
  onToggleFilters: () => void;
  columns?: QueryColumn[];
  rows?: unknown[][];
  onFetchAllRows?: () => Promise<{ columns: QueryColumn[]; rows: unknown[][] }>;
}

export default function TableViewHeader({
  schema, table, totalRows, totalRowsEstimated, counting, onCountExactly, hasNoPk, readOnly,
  selectedCount, deleting, onDelete,
  hasPendingChanges, dirtyRowCount, saveError, saving, onSave, onDiscard, onReviewSql,
  refreshing, loading, spinKey, onRefresh,
  hasCustomWidths, onResetWidths,
  onInsertRow, filterCount, filtersOpen, onToggleFilters,
  columns, rows, onFetchAllRows,
}: TableViewHeaderProps) {
  return (
    <PageHeader className="justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <HeaderTitle label={`Table · ${schema}`}>{table}</HeaderTitle>
        {totalRows !== undefined &&
          (totalRowsEstimated ? (
            <button
              type="button"
              onClick={onCountExactly}
              disabled={counting}
              aria-label="Estimated row count, click for an exact count"
              className="flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-xs tabular-nums text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground disabled:cursor-default"
            >
              {counting ? <Loader size={11} className="animate-spin" /> : "~"}
              {totalRows.toLocaleString()} rows
            </button>
          ) : (
            <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs tabular-nums text-muted-foreground">
              {totalRows.toLocaleString()} rows
            </span>
          ))}
        {readOnly ? (
          <span className="flex items-center gap-1 text-sm text-warning-foreground/80">
            <Lock size={12} />
            read-only connection
          </span>
        ) : hasNoPk ? (
          <span className="text-sm text-warning-foreground/80">
            no primary key - editing disabled
          </span>
        ) : null}
      </div>

      <div className="flex items-center gap-2">
        {selectedCount > 0 && !hasNoPk && !readOnly && (
          <Button variant="destructive" size="sm" onClick={onDelete} disabled={deleting}>
            <Trash2 />
            {deleting
              ? "Deleting…"
              : `Delete ${selectedCount} row${selectedCount > 1 ? "s" : ""}`}
          </Button>
        )}

        {hasPendingChanges && (
          <div className="flex items-center gap-2">
            {saveError && (
              <span className="text-sm text-destructive-foreground">{saveError}</span>
            )}
            <span className="text-sm text-muted-foreground">
              {dirtyRowCount} row{dirtyRowCount > 1 ? "s" : ""} changed
            </span>
            <Button variant="ghost-muted" size="sm" onClick={onReviewSql}>
              <FileCode /> SQL
            </Button>
            <Button variant="ghost-muted" size="sm" onClick={onDiscard}>
              <X /> Discard
            </Button>
            <Button variant="primary" size="sm" onClick={onSave} disabled={saving}>
              <Save />
              {saving ? "Saving…" : "Save (⌘S)"}
            </Button>
          </div>
        )}

        {!readOnly && (
          <Button variant="ghost-muted" size="sm" onClick={onInsertRow}>
            <Plus />
            Insert
          </Button>
        )}

        <Button
          variant="ghost-muted"
          size="sm"
          data-active={filtersOpen || filterCount > 0}
          onClick={onToggleFilters}
        >
          <ListFilter />
          Filter
          {filterCount > 0 && <Badge variant="info">{filterCount}</Badge>}
        </Button>

        <Button variant="ghost-muted" size="icon-sm" onClick={onRefresh} disabled={refreshing || loading} aria-label="Refresh">
          <RefreshCw
            key={spinKey}
            className={
              refreshing
                ? "animate-spin"
                : spinKey > 0
                  ? "animate-[spin_0.5s_ease-in-out]"
                  : ""
            }
          />
        </Button>

        {hasCustomWidths && (
          <Button variant="ghost-muted" size="icon-sm" onClick={onResetWidths} aria-label="Reset column widths">
            <Columns3 />
          </Button>
        )}

        {columns && rows && columns.length > 0 && (
          <ExportMenu
            columns={columns}
            rows={rows}
            defaultName={`${schema}.${table}`}
            totalRows={totalRows}
            totalRowsEstimated={totalRowsEstimated}
            onFetchAll={onFetchAllRows}
          />
        )}

        <div className={segmentedListClass}>
          <NavLink to={`/db/${schema}/${table}`} end className={({ isActive }) => segmentedItemClass(isActive)}>
            Data
          </NavLink>
          <NavLink to={`/db/${schema}/${table}/structure`} className={({ isActive }) => segmentedItemClass(isActive)}>
            Structure
          </NavLink>
        </div>
      </div>
    </PageHeader>
  );
}
