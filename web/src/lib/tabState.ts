// State that must survive a page unmounting inside a window tab (e.g. the query
// editor's tabs while you look at a table), kept separately for each tab.
// A plain module map instead of React state: it is read during render and
// mutated from event handlers, like the module variables it replaces.
const stores = new Map<string, Map<string, unknown>>();

export function tabState<T>(tabId: string, key: string, init: () => T): T {
  let store = stores.get(tabId);
  if (!store) {
    store = new Map();
    stores.set(tabId, store);
  }
  if (!store.has(key)) store.set(key, init());
  return store.get(key) as T;
}

export function disposeTabState(tabId: string) {
  stores.delete(tabId);
}
