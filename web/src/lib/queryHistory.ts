/** One run of the query editor, kept per connection in this computer's local storage. */
export interface HistoryEntry {
  id: string;
  sql: string;
  /** ISO timestamp of the (latest) run. */
  ranAt: string;
  durationMs: number | null;
  /** Rows returned, or rows affected for statements without results. */
  rows: number | null;
  /** The run failed; holds the error's first line. */
  error: string | null;
}

const MAX_ENTRIES = 500;
const PREFIX = "dbunny:history:";

/** History is shared by every tab connected to the same database. */
export function historyKey(info: { host: string; port: number; database: string }) {
  return `${PREFIX}${info.host}:${info.port}/${info.database}`;
}

export function loadHistory(key: string): HistoryEntry[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function save(key: string, entries: HistoryEntry[]) {
  try {
    localStorage.setItem(key, JSON.stringify(entries.slice(0, MAX_ENTRIES)));
  } catch {
    // Storage full or unavailable: history is a convenience, the run itself already happened
  }
}

/** Adds a run at the top; running the same SQL again moves it up instead of repeating it. */
export function addToHistory(key: string, entry: Omit<HistoryEntry, "id" | "ranAt">): HistoryEntry[] {
  const sql = entry.sql.trim();
  const rest = loadHistory(key).filter((e) => e.sql !== sql);
  const next = [{ ...entry, sql, id: crypto.randomUUID(), ranAt: new Date().toISOString() }, ...rest];
  save(key, next);
  return next;
}

export function removeFromHistory(key: string, id: string): HistoryEntry[] {
  const next = loadHistory(key).filter((e) => e.id !== id);
  save(key, next);
  return next;
}

export function clearHistory(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    // nothing to clear
  }
}

/** First line of an error from the backend (plain text or the JSON error object). */
export function errorSummary(raw: string): string {
  try {
    const parsed = JSON.parse(raw);
    if (typeof parsed?.message === "string") return parsed.message;
  } catch {
    // plain text
  }
  return raw.split("\n")[0];
}
