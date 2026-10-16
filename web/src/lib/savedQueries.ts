/** A named query kept per database in this computer's local storage. */
export interface SavedQuery {
  id: string;
  name: string;
  sql: string;
  updatedAt: string;
}

const PREFIX = "dbunny:saved:";

export function savedKey(info: { host: string; port: number; database: string }) {
  return `${PREFIX}${info.host}:${info.port}/${info.database}`;
}

export function loadSaved(key: string): SavedQuery[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function write(key: string, list: SavedQuery[]) {
  try {
    localStorage.setItem(key, JSON.stringify(list));
  } catch {
    // storage full or unavailable
  }
}

/** Saves a new query, or updates the one with `id`; the list stays sorted by name. */
export function saveQuery(key: string, query: { id?: string; name: string; sql: string }): { list: SavedQuery[]; saved: SavedQuery } {
  const saved: SavedQuery = {
    id: query.id ?? crypto.randomUUID(),
    name: query.name.trim(),
    sql: query.sql,
    updatedAt: new Date().toISOString(),
  };
  const list = [...loadSaved(key).filter((q) => q.id !== saved.id), saved].sort((a, b) => a.name.localeCompare(b.name));
  write(key, list);
  return { list, saved };
}

export function removeSaved(key: string, id: string): SavedQuery[] {
  const list = loadSaved(key).filter((q) => q.id !== id);
  write(key, list);
  return list;
}
