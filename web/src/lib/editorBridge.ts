import { useSyncExternalStore } from "react";

/** What the assistant can do with the SQL editor in front. */
export interface EditorHandle {
  /** Writes `text` at the cursor, in place of the selection if there is one. */
  insert(text: string): void;
  /** Replaces the selection with `text`. */
  replaceSelection(text: string): void;
  hasSelection(): boolean;
}

interface Entry {
  documentId: string;
  handle: EditorHandle;
}

let editors: Entry[] = [];
const listeners = new Set<() => void>();

function changed() {
  listeners.forEach((listener) => listener());
}

/**
 * Makes `handle` the editor the assistant writes to, until the returned function is called.
 * The last editor registered is the one in use; one registered again for the same document
 * replaces its earlier handle.
 */
export function registerEditor(documentId: string, handle: EditorHandle): () => void {
  const entry: Entry = { documentId, handle };
  editors = [...editors.filter((e) => e.documentId !== documentId), entry];
  changed();
  return () => {
    // Only this registration: a newer one for the same document stays
    if (editors.includes(entry)) {
      editors = editors.filter((e) => e !== entry);
      changed();
    }
  };
}

/** The editor in front, if there is one. */
export function activeEditor(): EditorHandle | null {
  return editors.length > 0 ? editors[editors.length - 1].handle : null;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useActiveEditor(): EditorHandle | null {
  return useSyncExternalStore(subscribe, activeEditor);
}
