import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { bindOpenQuery } from "../lib/openQuery";
import {
  closeDocument as closeIn,
  closeDocuments as closeManyIn,
  dropRelation as dropRelationIn,
  dropSchema as dropSchemaIn,
  emptyDocuments,
  focusDocument as focusIn,
  openDocument as openIn,
  renameQuery as renameQueryIn,
  setQuerySaved as setQuerySavedIn,
  updateCreateTable as updateCreateTableIn,
  renameRelation as renameRelationIn,
  renameSchema as renameSchemaIn,
  seedQuery,
  type DocumentsState,
  type OpenTarget,
} from "../lib/documents";
import { useTab } from "./workspace";

interface DocumentsValue {
  documents: DocumentsState;
  openDocument: (target: OpenTarget) => void;
  focusDocument: (id: string) => void;
  closeDocument: (id: string) => void;
  closeDocuments: (ids: string[]) => void;
  renameQuery: (id: string, title: string) => void;
  updateCreateTable: (id: string, patch: { title?: string; schema?: string }) => void;
  setQuerySaved: (id: string, savedId: string, title: string) => void;
  renameRelation: (schema: string, name: string, newName: string) => void;
  renameSchema: (schema: string, newName: string) => void;
  dropRelation: (schema: string, name: string) => void;
  dropSchema: (schema: string) => void;
  /** The open data tab for this table reloads. No-op when it is not open. */
  refreshData: (schema: string, name: string) => void;
  registerDataRefresh: (key: string, refresh: () => void) => () => void;
  dirtyIds: ReadonlySet<string>;
  setQueryDirty: (id: string, dirty: boolean) => void;
}

const DocumentsContext = createContext<DocumentsValue | null>(null);

export function DocumentsProvider({ children }: { children: ReactNode }) {
  const { tabId } = useTab();
  const [documents, setDocuments] = useState(() => seedQuery(emptyDocuments()));
  const [dirtyIds, setDirtyIds] = useState<ReadonlySet<string>>(() => new Set());
  const refreshers = useRef(new Map<string, () => void>());

  const openDocument = useCallback((target: OpenTarget) => setDocuments((state) => openIn(state, target)), []);
  const focusDocument = useCallback((id: string) => setDocuments((state) => focusIn(state, id)), []);
  const forgetDirty = useCallback((ids: string[]) => {
    setDirtyIds((prev) => {
      if (!ids.some((id) => prev.has(id))) return prev;
      const next = new Set(prev);
      for (const id of ids) next.delete(id);
      return next;
    });
  }, []);
  const closeDocument = useCallback(
    (id: string) => {
      setDocuments((state) => closeIn(state, id));
      forgetDirty([id]);
    },
    [forgetDirty],
  );
  const closeDocuments = useCallback(
    (ids: string[]) => {
      setDocuments((state) => closeManyIn(state, ids));
      forgetDirty(ids);
    },
    [forgetDirty],
  );
  const renameQuery = useCallback((id: string, title: string) => setDocuments((state) => renameQueryIn(state, id, title)), []);
  const updateCreateTable = useCallback(
    (id: string, patch: { title?: string; schema?: string }) => setDocuments((state) => updateCreateTableIn(state, id, patch)),
    [],
  );
  const setQuerySaved = useCallback(
    (id: string, savedId: string, title: string) => setDocuments((state) => setQuerySavedIn(state, id, savedId, title)),
    [],
  );
  const renameRelation = useCallback(
    (schema: string, name: string, newName: string) => setDocuments((state) => renameRelationIn(state, schema, name, newName)),
    [],
  );
  const renameSchema = useCallback((schema: string, newName: string) => setDocuments((state) => renameSchemaIn(state, schema, newName)), []);
  const dropRelation = useCallback((schema: string, name: string) => setDocuments((state) => dropRelationIn(state, schema, name)), []);
  const dropSchema = useCallback((schema: string) => setDocuments((state) => dropSchemaIn(state, schema)), []);
  const refreshData = useCallback((schema: string, name: string) => {
    refreshers.current.get(`${schema}.${name}`)?.();
  }, []);
  const registerDataRefresh = useCallback((key: string, refresh: () => void) => {
    refreshers.current.set(key, refresh);
    return () => {
      if (refreshers.current.get(key) === refresh) refreshers.current.delete(key);
    };
  }, []);
  const setQueryDirty = useCallback((id: string, dirty: boolean) => {
    setDirtyIds((prev) => {
      const has = prev.has(id);
      if (has === dirty) return prev;
      const next = new Set(prev);
      if (dirty) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);

  useEffect(() => bindOpenQuery(tabId, (sql) => openDocument({ kind: "query", sql })), [tabId, openDocument]);

  const value = useMemo(
    () => ({
      documents, openDocument, focusDocument, closeDocument, closeDocuments, renameQuery, updateCreateTable, setQuerySaved, renameRelation, renameSchema,
      dropRelation, dropSchema, refreshData, registerDataRefresh, dirtyIds, setQueryDirty,
    }),
    [documents, openDocument, focusDocument, closeDocument, closeDocuments, renameQuery, updateCreateTable, setQuerySaved, renameRelation, renameSchema, dropRelation, dropSchema, refreshData, registerDataRefresh, dirtyIds, setQueryDirty],
  );

  return <DocumentsContext.Provider value={value}>{children}</DocumentsContext.Provider>;
}

export function useDocuments() {
  const ctx = useContext(DocumentsContext);
  if (!ctx) throw new Error("useDocuments must be used within DocumentsProvider");
  return ctx;
}
