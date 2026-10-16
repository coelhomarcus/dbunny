import { useMemo, useState, type CSSProperties } from "react";
import { ArrowRight, Database, Plus, Search, Trash2 } from "lucide-react";
import type { SavedConnection } from "@/types";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Badge from "../ui/Badge";

const DEFAULT_COLOR = "#3b82f6";

/** Rounded tile tinted with the connection colour: the row's visual anchor. */
function ConnectionIcon({ color }: { color: string }) {
  const style: CSSProperties = {
    color,
    backgroundColor: `color-mix(in srgb, ${color} 16%, transparent)`,
    boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 22%, transparent)`,
  };
  return (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-lg [&_svg]:size-4" style={style}>
      <Database />
    </span>
  );
}

function ConnectionRow({
  conn,
  selected,
  onSelect,
  onConnect,
  onDelete,
}: {
  conn: SavedConnection;
  selected: boolean;
  onSelect: () => void;
  onConnect: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <div className="flex h-12 items-center justify-between gap-2 rounded-lg bg-destructive/8 px-2.5">
        <span className="truncate text-xs text-destructive-foreground">Delete “{conn.name || "Unnamed"}”?</span>
        <div className="flex shrink-0 gap-1">
          <Button variant="destructive" size="xs" onClick={() => { onDelete(); setConfirming(false); }}>
            Delete
          </Button>
          <Button variant="ghost-muted" size="xs" onClick={() => setConfirming(false)}>
            Cancel
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={selected}
      onClick={onSelect}
      onDoubleClick={onConnect}
      onKeyDown={(e) => {
        if (e.key === "Enter") onConnect();
      }}
      className={cn(
        "group relative flex h-12 items-center gap-2.5 rounded-lg px-2 cursor-pointer outline-none",
        "focus-visible:ring-2 focus-visible:ring-ring/50",
        selected ? "bg-accent" : "hover:bg-muted",
      )}
    >
      <ConnectionIcon color={conn.color || DEFAULT_COLOR} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium leading-snug">{conn.name || "Unnamed"}</div>
        <div className="truncate text-xs leading-snug text-muted-foreground">{conn.database}</div>
      </div>
      <div className="absolute right-1.5 flex items-center gap-0.5 rounded-md opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
        <Button
          variant="ghost-muted"
          size="icon-xs"
          className="bg-chrome"
          aria-label="Connect"
          onClick={(e) => { e.stopPropagation(); onConnect(); }}
        >
          <ArrowRight />
        </Button>
        <Button
          variant="ghost-muted"
          size="icon-xs"
          className="bg-chrome hover:bg-destructive/10 hover:text-destructive-foreground"
          aria-label="Delete"
          onClick={(e) => { e.stopPropagation(); setConfirming(true); }}
        >
          <Trash2 />
        </Button>
      </div>
    </div>
  );
}

/** Saved connections of the home screen: search, select, connect, delete. */
export default function ConnectionSidebar({
  connections,
  selectedId,
  expanded,
  onSelect,
  onConnect,
  onDelete,
  onNew,
}: {
  /** null while loading */
  connections: SavedConnection[] | null;
  selectedId: string | null;
  expanded: boolean;
  onSelect: (conn: SavedConnection) => void;
  onConnect: (conn: SavedConnection) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
}) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!connections || !q) return connections ?? [];
    return connections.filter((c) =>
      [c.name, c.host, c.database, c.user].some((v) => v?.toLowerCase().includes(q)),
    );
  }, [connections, query]);
  const count = connections?.length ?? 0;

  return (
    // Open or hidden, nothing in between; hidden, it also gives back the gap
    <aside
      inert={!expanded}
      className={cn(
        "relative shrink-0 select-none overflow-hidden rounded-lg bg-sidebar transition-[width,margin] duration-300 ease-in-out",
        expanded ? "w-64 border border-border" : "-mr-1.5 w-0",
      )}
    >
      <div
        className={cn(
          "absolute inset-y-0 left-0 flex w-64 flex-col transition-opacity duration-200",
          expanded ? "opacity-100 delay-75" : "opacity-0",
        )}
      >
        <div className="flex h-10 shrink-0 items-center gap-1 pl-3 pr-2">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <span className="text-[13px] font-semibold">Connections</span>
            {count > 0 && <Badge>{count}</Badge>}
          </div>
          <Button variant="ghost-muted" size="icon-sm" onClick={onNew} aria-label="New connection">
            <Plus />
          </Button>
        </div>

        {count > 0 && (
          <div className="relative px-2">
            <Search size={13} className="pointer-events-none absolute left-4.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setQuery("")}
              placeholder="Search"
              spellCheck={false}
              className="h-8 w-full rounded-lg bg-muted pl-7 pr-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground transition-shadow focus-visible:ring-2 focus-visible:ring-ring/40"
            />
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-2">
          {connections === null ? null : count === 0 ? (
            <div className="flex flex-col items-center px-4 pt-10 text-center">
              <div className="mb-3 flex size-10 items-center justify-center rounded-xl bg-muted text-muted-foreground">
                <Database size={18} />
              </div>
              <p className="text-sm font-medium">No saved connections</p>
              <p className="mt-1 text-xs text-muted-foreground">Save a connection from the form to reach it in one click.</p>
            </div>
          ) : filtered.length === 0 ? (
            <p className="px-2 pt-6 text-center text-xs text-muted-foreground">
              Nothing matches “<span className="text-foreground">{query}</span>”
            </p>
          ) : (
            <div className="flex flex-col gap-0.5">
              {filtered.map((conn) => (
                <ConnectionRow
                  key={conn.id}
                  conn={conn}
                  selected={conn.id === selectedId}
                  onSelect={() => onSelect(conn)}
                  onConnect={() => onConnect(conn)}
                  onDelete={() => onDelete(conn.id)}
                />
              ))}
            </div>
          )}
        </div>
      </div>

    </aside>
  );
}
