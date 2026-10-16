import { useMemo, useState } from "react";
import { History, Search, Trash2, X } from "lucide-react";
import type { HistoryEntry } from "../../lib/queryHistory";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import { inputClass } from "../ui/styles";

function dayLabel(date: Date) {
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: date.getFullYear() === today.getFullYear() ? undefined : "numeric" });
}

/** Past runs on this database, newest first; clicking one opens it in a new query tab. */
export default function HistoryPanel({
  entries,
  onOpen,
  onRemove,
  onClear,
  onClose,
}: {
  entries: HistoryEntry[];
  onOpen: (sql: string) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  onClose: () => void;
}) {
  const [search, setSearch] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);

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
    <aside className="flex h-full w-80 shrink-0 flex-col border-l border-border bg-card" aria-label="Query history">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3 select-none">
        <History size={14} className="text-muted-foreground" />
        <span className="flex-1 text-[13px] font-medium">History</span>
        {entries.length > 0 &&
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
            <Button variant="ghost-muted" size="icon-xs" aria-label="Clear history" onClick={() => setConfirmClear(true)}>
              <Trash2 />
            </Button>
          ))}
        <Button variant="ghost-muted" size="icon-xs" aria-label="Close history" onClick={onClose}>
          <X />
        </Button>
      </div>

      {entries.length > 0 && (
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
        {entries.length === 0 ? (
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
