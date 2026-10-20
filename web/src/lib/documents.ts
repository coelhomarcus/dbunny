import type { TableFilter } from "../types/query";
import { t } from "../i18n/store.ts";

export type DataIcon = "table" | "view" | "matview" | "partition";

export type DocumentKind =
  | "query"
  | "data"
  | "structure"
  | "activity"
  | "roles"
  | "function"
  | "procedure"
  | "sequence"
  | "type"
  | "index"
  | "trigger"
  | "create-table"
  | "diagram"
  | "assistant";

export interface FilterRequest {
  id: number;
  filters: TableFilter[];
}

export interface Document {
  id: string;
  kind: DocumentKind;
  title: string;
  schema?: string;
  name?: string;
  detail?: string;
  savedId?: string;
  icon?: DataIcon;
  sql?: string;
  filterRequest?: FilterRequest;
}

export interface DocumentsState {
  items: Document[];
  activeId: string | null;
  nextQuery: number;
  nextId: number;
}

export type OpenTarget =
  | { kind: "query"; sql?: string; savedId?: string; title?: string }
  | { kind: "data"; schema: string; name: string; icon: DataIcon; filters?: TableFilter[] }
  | { kind: "structure"; schema: string; name: string }
  | { kind: "activity" }
  | { kind: "roles" }
  | {
      kind: "function" | "procedure" | "sequence" | "type" | "index" | "trigger";
      schema: string;
      name: string;
      detail?: string;
    }
  | { kind: "create-table"; schema: string }
  | { kind: "diagram"; schema: string }
  | { kind: "assistant" };

export function emptyDocuments(): DocumentsState {
  return { items: [], activeId: null, nextQuery: 1, nextId: 1 };
}

function sameIdentity(doc: Document, target: Exclude<OpenTarget, { kind: "query" } | { kind: "create-table" }>): boolean {
  if (doc.kind !== target.kind) return false;
  if (target.kind === "activity" || target.kind === "roles" || target.kind === "assistant") return true;
  if (target.kind === "diagram") return doc.schema === target.schema;
  if (doc.schema !== target.schema || doc.name !== target.name) return false;
  if (target.kind === "function" || target.kind === "procedure") return (doc.detail ?? "") === (target.detail ?? "");
  return true;
}

function allocate(state: DocumentsState): { id: string; nextId: number } {
  return { id: `d${state.nextId}`, nextId: state.nextId + 1 };
}

export function openDocument(state: DocumentsState, target: OpenTarget): DocumentsState {
  if (target.kind === "query" && target.savedId) {
    const existing = state.items.find((doc) => doc.kind === "query" && doc.savedId === target.savedId);
    if (existing) return { ...state, activeId: existing.id };
  }
  if (target.kind !== "query" && target.kind !== "create-table") {
    const existing = state.items.find((doc) => sameIdentity(doc, target));
    if (existing) {
      if (target.kind === "data" && target.filters) {
        const request = { id: (existing.filterRequest?.id ?? 0) + 1, filters: target.filters };
        return {
          ...state,
          activeId: existing.id,
          items: state.items.map((doc) => (doc.id === existing.id ? { ...doc, filterRequest: request } : doc)),
        };
      }
      return { ...state, activeId: existing.id };
    }
  }

  const { id, nextId } = allocate(state);
  const titled = target.kind === "query" && target.title !== undefined;
  const title =
    target.kind === "query" ? target.title ?? t("ui.documents.query", { n: state.nextQuery })
    : target.kind === "create-table" ? t("ui.documents.newTable")
    : target.kind === "diagram" ? target.schema
    : target.kind === "activity" ? t("ui.documents.activity")
    : target.kind === "roles" ? t("ui.documents.roles")
    : target.kind === "assistant" ? t("ui.documents.assistant")
    : target.name;
  const doc: Document = { id, kind: target.kind, title };
  if (target.kind === "query") {
    doc.sql = target.sql ?? "";
    if (target.savedId) doc.savedId = target.savedId;
  } else if (target.kind === "create-table" || target.kind === "diagram") {
    doc.schema = target.schema;
  } else if (target.kind !== "activity" && target.kind !== "roles" && target.kind !== "assistant") {
    doc.schema = target.schema;
    doc.name = target.name;
    if (target.kind === "function" || target.kind === "procedure") doc.detail = target.detail ?? "";
    if (target.kind === "data") {
      doc.icon = target.icon;
      if (target.filters) doc.filterRequest = { id: 1, filters: target.filters };
    }
  }
  return {
    ...state,
    items: [...state.items, doc],
    activeId: id,
    nextId,
    nextQuery: titled || target.kind !== "query" ? state.nextQuery : state.nextQuery + 1,
  };
}

export function focusDocument(state: DocumentsState, id: string): DocumentsState {
  if (!state.items.some((doc) => doc.id === id)) return state;
  return { ...state, activeId: id };
}

export function closeDocument(state: DocumentsState, id: string): DocumentsState {
  return closeDocuments(state, [id]);
}

/** Closes every listed tab. The front tab moves to the nearest one that stays open. */
export function closeDocuments(state: DocumentsState, ids: string[]): DocumentsState {
  const drop = new Set(ids);
  if (!state.items.some((doc) => drop.has(doc.id))) return state;
  const items = state.items.filter((doc) => !drop.has(doc.id));
  if (!state.activeId || !drop.has(state.activeId)) return { ...state, items };
  const index = state.items.findIndex((doc) => doc.id === state.activeId);
  let neighbor: Document | undefined;
  for (let i = index + 1; i < state.items.length; i++) {
    if (!drop.has(state.items[i].id)) {
      neighbor = state.items[i];
      break;
    }
  }
  if (!neighbor) {
    for (let i = index - 1; i >= 0; i--) {
      if (!drop.has(state.items[i].id)) {
        neighbor = state.items[i];
        break;
      }
    }
  }
  return { ...state, items, activeId: neighbor?.id ?? null };
}

export function setQuerySaved(state: DocumentsState, id: string, savedId: string, title: string): DocumentsState {
  return {
    ...state,
    items: state.items.map((doc) => (doc.id === id && doc.kind === "query" ? { ...doc, savedId, title } : doc)),
  };
}

export function updateCreateTable(
  state: DocumentsState,
  id: string,
  patch: { title?: string; schema?: string },
): DocumentsState {
  return {
    ...state,
    items: state.items.map((doc) => {
      if (doc.id !== id || doc.kind !== "create-table") return doc;
      const next = { ...doc };
      if (patch.title !== undefined) {
        const trimmed = patch.title.trim();
        next.title = trimmed || t("ui.documents.newTable");
      }
      if (patch.schema !== undefined && patch.schema.trim()) next.schema = patch.schema.trim();
      return next;
    }),
  };
}

export function renameQuery(state: DocumentsState, id: string, title: string): DocumentsState {
  const trimmed = title.trim();
  if (!trimmed) return state;
  return {
    ...state,
    items: state.items.map((doc) => (doc.id === id && doc.kind === "query" ? { ...doc, title: trimmed } : doc)),
  };
}

export function renameRelation(state: DocumentsState, schema: string, name: string, newName: string): DocumentsState {
  return {
    ...state,
    items: state.items.map((doc) =>
      (doc.kind === "data" || doc.kind === "structure") && doc.schema === schema && doc.name === name
        ? { ...doc, name: newName, title: newName }
        : doc,
    ),
  };
}

export function renameSchema(state: DocumentsState, schema: string, newName: string): DocumentsState {
  return {
    ...state,
    items: state.items.map((doc) => {
      if (doc.schema !== schema) return doc;
      const next = { ...doc, schema: newName };
      if (doc.kind === "diagram") next.title = newName;
      return next;
    }),
  };
}

function without(state: DocumentsState, drop: (doc: Document) => boolean): DocumentsState {
  const removed = new Set(state.items.filter(drop).map((doc) => doc.id));
  if (removed.size === 0) return state;
  let next = state;
  for (const id of removed) next = closeDocument(next, id);
  return next;
}

export function dropRelation(state: DocumentsState, schema: string, name: string): DocumentsState {
  return without(state, (doc) => (doc.kind === "data" || doc.kind === "structure") && doc.schema === schema && doc.name === name);
}

export function dropSchema(state: DocumentsState, schema: string): DocumentsState {
  return without(state, (doc) => doc.schema === schema);
}

export function seedQuery(state: DocumentsState): DocumentsState {
  if (state.items.length > 0) return state;
  return openDocument(state, { kind: "query", sql: "SELECT 1;" });
}
