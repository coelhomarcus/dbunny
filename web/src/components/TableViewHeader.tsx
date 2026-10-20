import { Save, X, Trash2, ListFilter, Lock, FileCode, Plus, KeyRound } from "lucide-react";
import Button from "./ui/Button";
import Badge from "./ui/Badge";
import Tooltip from "./Tooltip";
import { HeaderTitle, PageHeader } from "./layout/PageHeader";
import type { RelationType } from "../lib/api";
import { Keys } from "./ui/Kbd";
import { useT } from "../i18n";

const LABEL: Record<RelationType, string> = {
  table: "Table",
  partitioned: "Partitioned table",
  foreign: "Foreign table",
  view: "View",
  matview: "Materialized view",
};

interface TableViewHeaderProps {
  schema: string;
  table: string;
  /** Null until it loads. */
  relationType: RelationType | null;
  hasNoPk: boolean;
  /** Read-only connection: no insert, edit or delete. */
  readOnly: boolean;
  selectedCount: number;
  deleting: boolean;
  onDelete: () => void;
  hasPendingChanges: boolean;
  dirtyRowCount: number;
  saving: boolean;
  onSave: () => void;
  onDiscard: () => void;
  /** Shows the SQL that saving would run. */
  onReviewSql: () => void;
  onInsertRow: () => void;
  filterCount: number;
  filtersOpen: boolean;
  onToggleFilters: () => void;
}

/**
 * The table's title and its main actions. Selected rows and unsaved edits take
 * over the actions while there are any; the rest of the tools live under the rows.
 */
export default function TableViewHeader({
  schema, table, relationType, hasNoPk, readOnly: connectionReadOnly,
  selectedCount, deleting, onDelete,
  hasPendingChanges, dirtyRowCount, saving, onSave, onDiscard, onReviewSql,
  onInsertRow, filterCount, filtersOpen, onToggleFilters,
}: TableViewHeaderProps) {
  const t = useT();
  const isView = relationType === "view" || relationType === "matview";
  const readOnly = connectionReadOnly || isView;
  const canDelete = selectedCount > 0 && !hasNoPk && !readOnly;
  return (
    <PageHeader className="justify-between">
      <div className="flex min-w-0 items-center gap-2.5">
        <HeaderTitle label={relationType ? `${LABEL[relationType]} · ${schema}` : schema}>{table}</HeaderTitle>
        {isView ? (
          <Tooltip
            label={relationType === "matview" ? t("ui.tableViewHeader.matviewReadOnly") : t("ui.tableViewHeader.viewReadOnly")}
            side="bottom"
          >
            <Badge variant="neutral">
              <Lock /> {t("ui.tableViewHeader.readOnly")}
            </Badge>
          </Tooltip>
        ) : connectionReadOnly ? (
          <Tooltip label={t("ui.tableViewHeader.readOnlyHint")} side="bottom">
            <Badge variant="warning">
              <Lock /> {t("ui.tableViewHeader.readOnly")}
            </Badge>
          </Tooltip>
        ) : hasNoPk ? (
          <Tooltip label={t("ui.tableViewHeader.noPrimaryKeyHint")} side="bottom">
            <Badge variant="warning">
              <KeyRound /> {t("ui.tableViewHeader.noPrimaryKey")}
            </Badge>
          </Tooltip>
        ) : null}
      </div>

      <div className="flex items-center gap-1.5">
        {hasPendingChanges ? (
          <>
            <span className="mr-1 text-xs text-muted-foreground tabular-nums">
              {t("ui.tableViewHeader.rowsChanged", { count: dirtyRowCount })}
            </span>
            <Button variant="ghost-muted" size="sm" onClick={onReviewSql}>
              <FileCode /> SQL
            </Button>
            <Button variant="ghost-muted" size="sm" onClick={onDiscard}>
              <X /> {t("ui.tableViewHeader.discard")}
            </Button>
            <Button variant="primary" size="sm" onClick={onSave} disabled={saving}>
              <Save />
              {saving ? t("ui.tableViewHeader.saving") : t("common.save")}
              <Keys keys="⌘S" tone="inverse" />
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost-muted" size="sm" data-active={filtersOpen || filterCount > 0} onClick={onToggleFilters}>
              <ListFilter />
              {t("ui.tableViewHeader.filter")}
              {filterCount > 0 && <Badge variant="info">{filterCount}</Badge>}
            </Button>
            {/* With rows selected, deleting them takes Insert's place */}
            {canDelete ? (
              <Button variant="destructive" size="sm" onClick={onDelete} disabled={deleting}>
                <Trash2 />
                {deleting ? t("ui.tableViewHeader.deleting") : t("ui.tableViewHeader.deleteRows", { count: selectedCount })}
              </Button>
            ) : (
              // Waits for the kind, so it never flashes on a view
              !readOnly && relationType && (
                <Button variant="ghost-muted" size="sm" onClick={onInsertRow}>
                  <Plus />
                  {t("ui.tableViewHeader.insert")}
                </Button>
              )
            )}
          </>
        )}

      </div>
    </PageHeader>
  );
}
