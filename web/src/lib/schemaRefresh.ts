/** Asks one connection's explorer to reload after a table is created. */
const listeners = new Map<string, (schema: string) => void>();

export function requestSchemaRefresh(tabId: string, schema: string) {
  listeners.get(tabId)?.(schema);
}

export function bindSchemaRefresh(tabId: string, refresh: (schema: string) => void) {
  listeners.set(tabId, refresh);
  return () => {
    if (listeners.get(tabId) === refresh) listeners.delete(tabId);
  };
}
