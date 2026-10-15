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
    <div className="flex flex-col select-none">
      {connections.map((conn) => {
        const isSelected = conn.id === selectedId;
        const isConfirmingDelete = conn.id === confirmingDeleteId;

        return (
          <div
            key={conn.id}
            className={`group relative px-3 h-12 flex items-center cursor-pointer transition-colors border-l-2 ${
              isSelected
                ? "bg-accent border-primary"
                : "border-transparent hover:bg-muted"
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
                <span className="text-xs text-muted-foreground">Delete?</span>
                <div className="flex gap-1.5">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete(conn.id);
                      setConfirmingDeleteId(null);
                    }}
                    className="px-2 py-0.5 rounded-md text-[11px] font-medium text-destructive-foreground hover:bg-destructive/10 transition-colors cursor-pointer"
                  >
                    Yes
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirmingDeleteId(null);
                    }}
                    className="px-2 py-0.5 rounded-md text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer"
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
                    <span className="text-sm text-foreground truncate">
                      {conn.name || "Unnamed"}
                    </span>
                  </div>
                  <p className="text-xs text-muted-foreground/70 truncate pl-4 font-mono">
                    {conn.host}:{conn.port}/{conn.database}
                  </p>
                </div>

                <div className="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 flex items-center gap-0.5 transition-opacity">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onConnect(conn);
                    }}
                    className="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer"
                    title="Connect"
                  >
                    <ArrowRight size={12} />
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirmingDeleteId(conn.id);
                    }}
                    className="p-1 rounded-md text-muted-foreground/70 hover:text-destructive-foreground hover:bg-destructive/10 transition-colors cursor-pointer"
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
