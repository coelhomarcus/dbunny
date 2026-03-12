import { NavLink } from "react-router";
import {
  Save,
  X,
  Trash2,
  RefreshCw,
  Columns3,
} from "lucide-react";

interface TableViewHeaderProps {
  schema: string;
  table: string;
  totalRows?: number;
  hasNoPk: boolean;
  selectedCount: number;
  deleting: boolean;
  onDelete: () => void;
  hasPendingChanges: boolean;
  dirtyRowCount: number;
  saveError: string | null;
  saving: boolean;
  onSave: () => void;
  onDiscard: () => void;
  refreshing: boolean;
  loading: boolean;
  spinKey: number;
  onRefresh: () => void;
  hasCustomWidths: boolean;
  onResetWidths: () => void;
}

export default function TableViewHeader({
  schema, table, totalRows, hasNoPk,
  selectedCount, deleting, onDelete,
  hasPendingChanges, dirtyRowCount, saveError, saving, onSave, onDiscard,
  refreshing, loading, spinKey, onRefresh,
  hasCustomWidths, onResetWidths,
}: TableViewHeaderProps) {
  return (
    <div className="flex items-center justify-between px-4 py-2 border-b border-zinc-800">
      <div className="flex items-center gap-3">
        <h2 className="text-sm font-medium">
          <span className="text-zinc-500">{schema}.</span>
          {table}
        </h2>
        {totalRows !== undefined && (
          <span className="text-xs text-zinc-500">
            {totalRows.toLocaleString()} rows
          </span>
        )}
        {hasNoPk && (
          <span className="text-xs text-amber-500/70">
            no primary key — editing disabled
          </span>
        )}
      </div>

      <div className="flex items-center gap-2">
        {selectedCount > 0 && !hasNoPk && (
          <button
            onClick={onDelete}
            disabled={deleting}
            className="flex items-center gap-1.5 px-3 py-1 text-xs bg-red-600/80 hover:bg-red-600 disabled:bg-zinc-700 text-white rounded transition-colors"
          >
            <Trash2 size={12} />
            {deleting
              ? "Deleting…"
              : `Delete ${selectedCount} row${selectedCount > 1 ? "s" : ""}`}
          </button>
        )}

        {hasPendingChanges && (
          <div className="flex items-center gap-2">
            {saveError && (
              <span className="text-xs text-red-400">{saveError}</span>
            )}
            <span className="text-xs text-zinc-500">
              {dirtyRowCount} row{dirtyRowCount > 1 ? "s" : ""} changed
            </span>
            <button
              onClick={onDiscard}
              className="flex items-center gap-1 px-2 py-1 text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
            >
              <X size={12} /> Discard
            </button>
            <button
              onClick={onSave}
              disabled={saving}
              className="flex items-center gap-1.5 px-3 py-1 text-xs bg-green-600 hover:bg-green-500 disabled:bg-zinc-700 text-white rounded transition-colors"
            >
              <Save size={12} />
              {saving ? "Saving…" : "Save (⌘S)"}
            </button>
          </div>
        )}

        <button
          onClick={onRefresh}
          disabled={refreshing || loading}
          title="Refresh"
          className="p-1.5 text-zinc-400 hover:text-zinc-200 disabled:opacity-40 transition-colors"
        >
          <RefreshCw
            key={spinKey}
            size={13}
            className={
              refreshing
                ? "animate-spin"
                : spinKey > 0
                  ? "animate-[spin_0.5s_ease-in-out]"
                  : ""
            }
          />
        </button>

        {hasCustomWidths && (
          <button
            onClick={onResetWidths}
            title="Reset column widths"
            className="p-1.5 text-zinc-400 hover:text-zinc-200 transition-colors"
          >
            <Columns3 size={13} />
          </button>
        )}

        <div className="flex gap-1">
          <NavLink
            to={`/db/${schema}/${table}`}
            end
            className={({ isActive }) =>
              `px-3 py-1 text-xs rounded transition-colors ${isActive ? "bg-zinc-700 text-white" : "text-zinc-400 hover:text-white"}`
            }
          >
            Data
          </NavLink>
          <NavLink
            to={`/db/${schema}/${table}/structure`}
            className={({ isActive }) =>
              `px-3 py-1 text-xs rounded transition-colors ${isActive ? "bg-zinc-700 text-white" : "text-zinc-400 hover:text-white"}`
            }
          >
            Structure
          </NavLink>
        </div>
      </div>
    </div>
  );
}
