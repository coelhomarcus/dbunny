import { ChevronsUpDown, ChevronUp, ChevronDown, KeyRound, Link2, Sparkles } from "lucide-react";
import type { ColumnInfo } from "@/types";
import { columnFacts, shortType } from "../lib/columnMeta";
import Tooltip from "./Tooltip";

interface SortableColumnHeaderProps {
  id: string;
  columnInfo?: ColumnInfo;
  sorted: false | "asc" | "desc";
  /** Position in a multi-column sort (0 = first), or -1 when it's the only sort key. */
  sortIndex: number;
  /** The whole column is selected. */
  selected: boolean;
  onSelect: () => void;
  onToggleSort: ((event: unknown) => void);
  onResizeStart: (e: React.MouseEvent) => void;
}

function ColumnIcon({ column, selected }: { column: ColumnInfo; selected: boolean }) {
  const color = selected ? "text-zinc-950" : undefined;
  if (column.isPrimaryKey) return <KeyRound size={12} className={`shrink-0 ${color ?? "text-warning-foreground"}`} aria-label="Primary key" />;
  if (column.foreignKey) return <Link2 size={12} className={`shrink-0 ${color ?? "text-info-foreground"}`} aria-label="Foreign key" />;
  if (column.isGenerated) return <Sparkles size={12} className={`shrink-0 ${color ?? "text-muted-foreground"}`} aria-label="Generated" />;
  return null;
}

export default function SortableColumnHeader({
  id, columnInfo, sorted, sortIndex, selected, onSelect, onToggleSort, onResizeStart,
}: SortableColumnHeaderProps) {
  const isActive = sorted !== false;
  const facts = columnInfo ? columnFacts(columnInfo) : [];

  const label = (
    <span className="flex items-center gap-1.5 min-w-0 overflow-hidden">
      {columnInfo && <ColumnIcon column={columnInfo} selected={selected} />}
      <span className={`truncate font-semibold ${selected ? "text-zinc-950" : "text-foreground"}`}>{id}</span>
      {columnInfo?.typeName && (
        <span className={`shrink-0 text-[11px] font-normal ${selected ? "text-zinc-800" : "text-muted-foreground"}`}>
          {shortType(columnInfo.typeName)}
        </span>
      )}
    </span>
  );

  return (
    <th
      data-grid-column={id}
      onClick={onSelect}
      className={`relative cursor-pointer select-none border-b border-r px-3 py-1.5 text-left text-xs ${
        selected
          ? "border-warning bg-warning shadow-[inset_2px_0_0_var(--warning),inset_-2px_0_0_var(--warning),inset_0_2px_0_var(--warning)]"
          : isActive
            ? "border-border bg-card-header-active"
            : "border-border bg-card-header"
      }`}
    >
      <span className="flex items-center justify-between gap-2 overflow-hidden">
        {facts.length > 0 ? (
          <Tooltip label={facts.join("\n")} side="bottom" className="min-w-0">
            {label}
          </Tooltip>
        ) : (
          label
        )}
        <button
          type="button"
          aria-label={sorted ? `Sort ${id}, ${sorted === "asc" ? "ascending" : "descending"}` : `Sort by ${id}`}
          onClick={(e) => {
            e.stopPropagation();
            onToggleSort(e);
          }}
          className={`flex shrink-0 cursor-pointer items-center gap-0.5 rounded px-0.5 transition-colors ${
            selected ? "text-zinc-950" : isActive ? "text-info-foreground" : "text-muted-foreground/70 hover:text-muted-foreground"
          }`}
        >
          {sorted === "asc" ? (
            <ChevronUp size={13} />
          ) : sorted === "desc" ? (
            <ChevronDown size={13} />
          ) : (
            <ChevronsUpDown size={13} />
          )}
          {sorted && sortIndex >= 0 && <span className="text-[10px] font-semibold tabular-nums">{sortIndex + 1}</span>}
        </button>
      </span>
      <div
        onMouseDown={onResizeStart}
        onClick={(e) => e.stopPropagation()}
        className="absolute right-0 top-0 h-full w-1.5 cursor-col-resize hover:bg-muted-foreground/40 active:bg-muted-foreground/60 transition-colors"
      />
    </th>
  );
}
