import {
  ChevronsUpDown,
  ChevronUp,
  ChevronDown,
} from "lucide-react";
import type { ColumnInfo } from "@/types";

interface SortableColumnHeaderProps {
  id: string;
  columnInfo?: ColumnInfo;
  sorted: false | "asc" | "desc";
  onToggleSort: ((event: unknown) => void);
  onResizeStart: (e: React.MouseEvent) => void;
}

function normalizeType(type: string): string {
  if (type.startsWith("timestamp")) return "timestamp";
  if (type.startsWith("time ")) return "time";
  if (type === "character varying") return "varchar";
  if (type === "double precision") return "float8";
  if (type === "character") return "char";
  return type;
}

export default function SortableColumnHeader({
  id, columnInfo, sorted, onToggleSort, onResizeStart,
}: SortableColumnHeaderProps) {
  const isActive = sorted !== false;

  return (
    <th
      onClick={onToggleSort}
      className={`relative text-left px-3 py-2.5 border-b border-r border-zinc-700/40 cursor-pointer hover:bg-zinc-700 select-none group text-sm ${
        isActive ? "bg-zinc-700" : "bg-zinc-800"
      }`}
    >
      <span className="flex items-center justify-between gap-2 overflow-hidden">
        <span className="flex items-center gap-1.5 min-w-0 overflow-hidden">
          {columnInfo?.isPrimaryKey && (
            <span className="text-xs text-amber-500/70 font-mono shrink-0">
              PK
            </span>
          )}
          <span className={`font-semibold truncate ${isActive ? "text-white" : "text-zinc-200"}`}>
            {id}
          </span>
          {columnInfo?.dataType && (
            <span className="text-zinc-500 shrink-0 font-normal text-xs">
              {normalizeType(columnInfo.dataType)}
            </span>
          )}
        </span>
        <span className={`shrink-0 transition-colors ${
          isActive
            ? "text-blue-400"
            : "text-zinc-600 group-hover:text-zinc-400"
        }`}>
          {sorted === "asc" ? (
            <ChevronUp size={13} />
          ) : sorted === "desc" ? (
            <ChevronDown size={13} />
          ) : (
            <ChevronsUpDown size={13} />
          )}
        </span>
      </span>
      <div
        onMouseDown={onResizeStart}
        onClick={(e) => e.stopPropagation()}
        className="absolute right-0 top-0 h-full w-1.5 cursor-col-resize hover:bg-zinc-500/40 active:bg-zinc-500/60 transition-colors"
      />
    </th>
  );
}
