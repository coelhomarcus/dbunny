import { useState } from "react";
import { Trash2, ArrowRight } from "lucide-react";
import type { SavedConnection } from "@/types";

interface SavedConnectionsListProps {
  connections: SavedConnection[];
  selectedId: string | null;
  onSelect: (connection: SavedConnection) => void;
  onConnect: (connection: SavedConnection) => void;
  onDelete: (id: string) => void;
}

export default function SavedConnectionsList({
  connections,
  selectedId,
  onSelect,
  onConnect,
  onDelete,
}: SavedConnectionsListProps) {
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(
    null
  );

  return (
    <div className="flex flex-col">
      {connections.map((conn) => {
        const isSelected = conn.id === selectedId;
        const isConfirmingDelete = conn.id === confirmingDeleteId;

        return (
          <div
            key={conn.id}
            className={`group relative px-3 h-12 flex items-center cursor-pointer transition-colors border-l-2 ${
              isSelected
                ? "bg-zinc-800/60 border-white"
                : "border-transparent hover:bg-zinc-800/30"
            }`}
            onClick={() => {
              if (!isConfirmingDelete) onSelect(conn);
            }}
            onDoubleClick={() => {
              if (!isConfirmingDelete) onConnect(conn);
            }}
          >
            {isConfirmingDelete ? (
              <div className="flex items-center justify-between w-full">
                <span className="text-xs text-zinc-400">Delete?</span>
                <div className="flex gap-1.5">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete(conn.id);
                      setConfirmingDeleteId(null);
                    }}
                    className="px-2 py-0.5 text-[11px] text-red-400 hover:text-red-300 transition-colors"
                  >
                    Yes
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirmingDeleteId(null);
                    }}
                    className="px-2 py-0.5 text-[11px] text-zinc-500 hover:text-zinc-300 transition-colors"
                  >
                    No
                  </button>
                </div>
              </div>
            ) : (
              <>
                <div className="min-w-0 flex-1 pr-14">
                  <div className="flex items-center gap-2">
                    <span
                      className="w-2 h-2 rounded-full shrink-0"
                      style={{ backgroundColor: conn.color || "#3b82f6" }}
                    />
                    <span className="text-[13px] text-zinc-200 truncate">
                      {conn.name || "Unnamed"}
                    </span>
                  </div>
                  <p className="text-[11px] text-zinc-600 truncate pl-4 font-mono">
                    {conn.host}:{conn.port}/{conn.database}
                  </p>
                </div>

                <div className="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 flex items-center gap-0.5 transition-opacity">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onConnect(conn);
                    }}
                    className="p-1 rounded text-zinc-500 hover:text-zinc-200 hover:bg-zinc-700/50 transition-colors"
                    title="Connect"
                  >
                    <ArrowRight size={12} />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirmingDeleteId(conn.id);
                    }}
                    className="p-1 rounded text-zinc-700 hover:text-red-400 hover:bg-zinc-700/50 transition-colors"
                    title="Delete"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
