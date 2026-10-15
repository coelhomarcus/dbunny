import { useMemo, useState, type CSSProperties } from "react";
import { ArrowRight, Database, PanelLeftClose, PanelLeftOpen, Plus, Search, Trash2 } from "lucide-react";
import type { SavedConnection } from "@/types";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import Badge from "../ui/Badge";

const DEFAULT_COLOR = "#3b82f6";

/** Rounded tile tinted with the connection colour: the row's visual anchor. */
function ConnectionIcon({ color, size = "md" }: { color: string; size?: "sm" | "md" }) {
  const style: CSSProperties = {
    color,
    backgroundColor: `color-mix(in srgb, ${color} 16%, transparent)`,
    boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 22%, transparent)`,
  };
  return (
    <span
      className={cn(
        "flex shrink-0 items-center justify-center rounded-lg",
        size === "md" ? "size-8 [&_svg]:size-4" : "size-7 [&_svg]:size-3.5",
      )}
      style={style}
    >
      <Database />
    </span>
  );
}

/** "db.acme.io · shop", with the port only when it isn't the default. */
function connectionSummary(conn: SavedConnection) {
  const host = conn.port && conn.port !== 5432 ? `${conn.host}:${conn.port}` : conn.host;
  return `${host} · ${conn.database}`;
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
      title="Double-click to connect"
      className={cn(
        "group relative flex h-12 items-center gap-2.5 rounded-lg px-2 cursor-pointer outline-none",
        "focus-visible:ring-2 focus-visible:ring-ring/50",
        selected ? "bg-accent" : "hover:bg-muted",
      )}
    >
      <ConnectionIcon color={conn.color || DEFAULT_COLOR} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium leading-snug">{conn.name || "Unnamed"}</div>
        <div className="truncate text-xs leading-snug text-muted-foreground">{connectionSummary(conn)}</div>
      </div>
      <div className="absolute right-1.5 flex items-center gap-0.5 rounded-md opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
        <Button
          variant="ghost-muted"
          size="icon-xs"
          className="bg-card hover:bg-card-hover"
          title="Connect"
          onClick={(e) => { e.stopPropagation(); onConnect(); }}
        >
          <ArrowRight />
        </Button>
        <Button
          variant="ghost-muted"
          size="icon-xs"
          className="bg-card hover:bg-destructive/10 hover:text-destructive-foreground"
          title="Delete"
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
  onToggleExpanded,
  onSelect,
  onConnect,
  onDelete,
  onNew,
}: {
  /** null while loading */
  connections: SavedConnection[] | null;
  selectedId: string | null;
  expanded: boolean;
  onToggleExpanded: () => void;
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
    <aside
      className={cn(
        // Divider starts below the 48px header row, so it never runs into the window tabs
        "relative shrink-0 after:pointer-events-none after:absolute after:right-0 after:top-12 after:bottom-0 after:w-px after:bg-border select-none overflow-hidden transition-[width] duration-300 ease-in-out",
        expanded ? "w-64" : "w-12",
      )}
    >
      {/* Expanded */}
      <div
        className={cn(
          "absolute inset-y-0 left-0 flex w-64 flex-col transition-opacity duration-200",
          expanded ? "opacity-100 delay-75" : "pointer-events-none opacity-0",
        )}
      >
        <div className="flex h-12 shrink-0 items-center gap-1 border-b border-border pl-3 pr-2">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <span className="text-sm font-semibold">Connections</span>
            {count > 0 && <Badge>{count}</Badge>}
          </div>
          <Button variant="ghost-muted" size="icon-sm" onClick={onNew} title="New connection">
            <Plus />
          </Button>
          <Button variant="ghost-muted" size="icon-sm" onClick={onToggleExpanded} title="Collapse sidebar">
            <PanelLeftClose />
          </Button>
        </div>

        {count > 0 && (
          <div className="relative px-2 pt-2">
            <Search size={13} className="pointer-events-none absolute left-4.5 top-1/2 mt-1 -translate-y-1/2 text-muted-foreground" />
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

      {/* Collapsed: icon rail, one dot per saved connection */}
      <div
        className={cn(
          "absolute inset-0 flex flex-col items-center transition-opacity duration-200",
          expanded ? "pointer-events-none opacity-0" : "opacity-100 delay-75",
        )}
      >
        <div className="flex h-12 w-full shrink-0 items-center justify-center border-b border-border">
          <Tooltip label="Expand sidebar">
            <Button variant="ghost-muted" size="icon-sm" onClick={onToggleExpanded} aria-label="Expand sidebar">
              <PanelLeftOpen />
            </Button>
          </Tooltip>
        </div>
        <div className="flex flex-col items-center gap-1.5 pt-2">
          <Tooltip label="New connection">
            <Button variant="ghost-muted" size="icon-sm" onClick={onNew} aria-label="New connection">
              <Plus />
            </Button>
          </Tooltip>
          {connections?.map((conn) => (
            <Tooltip key={conn.id} label={`Connect to ${conn.name || conn.database}`}>
              <button
                type="button"
                aria-label={conn.name}
                onClick={() => onConnect(conn)}
                className={cn(
                  "rounded-lg cursor-pointer outline-none transition-opacity hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring/50",
                  conn.id === selectedId && "ring-2 ring-ring/40 ring-offset-1 ring-offset-card",
                )}
              >
                <ConnectionIcon color={conn.color || DEFAULT_COLOR} size="sm" />
              </button>
            </Tooltip>
          ))}
        </div>
      </div>
    </aside>
  );
}
