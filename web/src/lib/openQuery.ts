/** SQL waiting for the query editor of one window tab. A hidden tab must not take it. */
const pending = new Map<string, string>();
const listeners = new Map<string, (sql: string) => void>();

/** Ask this window tab's query editor to open `sql` in a new tab. Does not run it. */
export function requestOpenQuery(tabId: string, sql: string) {
  const listener = listeners.get(tabId);
  if (listener) listener(sql);
  else pending.set(tabId, sql);
}

/** Deliver a request made before the editor was on screen, then later ones. */
export function bindOpenQuery(tabId: string, open: (sql: string) => void) {
  listeners.set(tabId, open);
  const sql = pending.get(tabId);
  if (sql !== undefined) {
    pending.delete(tabId);
    open(sql);
  }
  return () => {
    if (listeners.get(tabId) === open) listeners.delete(tabId);
  };
}
