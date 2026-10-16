import { useMemo, useState } from "react";
import { Search, Trash2, X } from "lucide-react";
import type { HistoryEntry } from "../../lib/queryHistory";
import type { SavedQuery } from "../../lib/savedQueries";
import SegmentedTabs from "../ui/SegmentedTabs";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import { inputClass } from "../ui/styles";

function dayLabel(date: Date) {
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: date.getFullYear() === today.getFullYear() ? undefined : "numeric" });
}

export type LibraryView = "history" | "saved";

/**
 * Saved queries and past runs on this database. Clicking one opens it in a new query
 * tab; a saved query's tab keeps the link, so saving it again updates it.
 */
export default function HistoryPanel({
  view,
  onViewChange,
  entries,
  saved,
  onOpen,
  onOpenSaved,
  onRemove,
  onRemoveSaved,
  onClear,
  onClose,
}: {
  view: LibraryView;
  onViewChange: (view: LibraryView) => void;
  entries: HistoryEntry[];
  saved: SavedQuery[];
  onOpen: (sql: string) => void;
  onOpenSaved: (query: SavedQuery) => void;
  onRemove: (id: string) => void;
  onRemoveSaved: (id: string) => void;
  onClear: () => void;
  onClose: () => void;
}) {
  const [search, setSearch] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);
  const shownSaved = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? saved.filter((s) => s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q)) : saved;
  }, [saved, search]);

  const groups = useMemo(() => {
    const q = search.trim().toLowerCase();
    const shown = q ? entries.filter((e) => e.sql.toLowerCase().includes(q)) : entries;
    const byDay = new Map<string, HistoryEntry[]>();
    for (const entry of shown) {
      const label = dayLabel(new Date(entry.ranAt));
      byDay.set(label, [...(byDay.get(label) ?? []), entry]);
    }
    return [...byDay.entries()];
  }, [entries, search]);

  return (
    <aside className="flex h-full min-h-0 w-full flex-col" aria-label="Query library">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3 select-none">
        <SegmentedTabs
          value={view}
          onChange={onViewChange}
          options={[
            { value: "saved", label: "Saved" },
            { value: "history", label: "History" },
          ]}
        />
        <span className="flex-1" />
        {view === "history" && entries.length > 0 &&
          (confirmClear ? (
            <>
              <Button variant="destructive-ghost" size="xs" onClick={() => { onClear(); setConfirmClear(false); }}>
                Clear all
              </Button>
              <Button variant="ghost-muted" size="xs" onClick={() => setConfirmClear(false)}>
                Keep
              </Button>
            </>
          ) : (
            <Tooltip label="Clear history" side="bottom">
              <Button variant="ghost-muted" size="icon-xs" aria-label="Clear history" onClick={() => setConfirmClear(true)}>
                <Trash2 />
              </Button>
            </Tooltip>
          ))}
        <Tooltip label="Close" side="bottom">
          <Button variant="ghost-muted" size="icon-xs" aria-label="Close history" onClick={onClose}>
            <X />
          </Button>
        </Tooltip>
      </div>

      {(view === "history" ? entries.length > 0 : saved.length > 0) && (
        <div className="relative shrink-0 border-b border-border p-2">
          <Search size={13} className="pointer-events-none absolute left-4.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search queries"
            aria-label="Search history"
            spellCheck={false}
            className={cn(inputClass, "h-7 pl-7 text-[13px]")}
          />
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {view === "saved" ? (
          saved.length === 0 ? (
            <p className="px-4 py-10 text-center text-sm text-muted-foreground">Save a query from the editor to keep it here.</p>
          ) : shownSaved.length === 0 ? (
            <p className="px-4 py-10 text-center text-sm text-muted-foreground">No query matches.</p>
          ) : (
            <ul className="py-1">
              {shownSaved.map((query) => (
                <li key={query.id} className="group relative">
                  <button
                    type="button"
                    onClick={() => onOpenSaved(query)}
                    className="block w-full px-3 py-2 text-left cursor-pointer hover:bg-accent focus-visible:bg-accent outline-none"
                  >
                    <span className="block truncate pr-6 text-[13px] font-medium">{query.name}</span>
                    <code className="mt-0.5 line-clamp-2 whitespace-pre-wrap break-words font-mono text-xs text-muted-foreground">{query.sql}</code>
                  </button>
                  <Button
                    variant="ghost-muted"
                    size="icon-xs"
                    aria-label={`Delete saved query ${query.name}`}
                    onClick={() => onRemoveSaved(query.id)}
                    className="absolute right-1.5 top-1.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  >
                    <Trash2 />
                  </Button>
                </li>
              ))}
            </ul>
          )
        ) : entries.length === 0 ? (
          <p className="px-4 py-10 text-center text-sm text-muted-foreground">Queries you run on this database show up here.</p>
        ) : groups.length === 0 ? (
          <p className="px-4 py-10 text-center text-sm text-muted-foreground">No query matches.</p>
        ) : (
          groups.map(([day, list]) => (
            <section key={day}>
              <h3 className="sticky top-0 z-[1] bg-card px-3 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-muted-foreground select-none">
                {day}
              </h3>
              <ul>
                {list.map((entry) => (
                  <li key={entry.id} className="group relative">
                    <button
                      type="button"
                      onClick={() => onOpen(entry.sql)}
                      className="block w-full px-3 py-2 text-left cursor-pointer hover:bg-accent focus-visible:bg-accent outline-none"
                    >
                      <code className="line-clamp-3 whitespace-pre-wrap break-words font-mono text-xs text-foreground">{entry.sql}</code>
                      <span className="mt-1 flex items-center gap-2 text-[11px] text-muted-foreground tabular-nums">
                        {new Date(entry.ranAt).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
                        {entry.error ? (
                          <span className="truncate text-destructive-foreground">{entry.error}</span>
                        ) : (
                          <>
                            {entry.rows !== null && <span>{entry.rows.toLocaleString()} rows</span>}
                            {entry.durationMs !== null && <span>{entry.durationMs} ms</span>}
                          </>
                        )}
                      </span>
                    </button>
                    <Button
                      variant="ghost-muted"
                      size="icon-xs"
                      aria-label="Remove from history"
                      onClick={() => onRemove(entry.id)}
                      className="absolute right-1.5 top-1.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                    >
                      <X />
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ))
        )}
      </div>
    </aside>
  );
}
