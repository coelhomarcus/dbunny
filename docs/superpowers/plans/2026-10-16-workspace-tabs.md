# Workspace document tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inside each connection tab, open tables, structure, queries, and the other screens as document tabs that stay mounted while hidden.

**Architecture:** A pure document list (`web/src/lib/documents.ts`) decides identity, focus, close, rename, and drop. A provider inside the connection pane holds that list in React state. The workspace renders one tab bar and a hidden stack of panes. Screens receive schema and name as props. The route only distinguishes the connect page from the connected workspace.

**Tech Stack:** React 19, TypeScript, React Router, Tailwind 4, Lucide, Node's built-in test runner.

## Global Constraints

- Connection tabs in the title bar stay. Several databases stay open. The database switcher still opens another connection tab.
- Documents are not written to disk. Disconnecting drops the list. Connecting seeds one query.
- A hidden document is `hidden` and `inert`. It receives no keyboard and no clicks.
- Closing a document does not ask for confirmation. The connection-tab close shortcut still closes the connection tab.
- Activity and Roles are offered only when `capabilities.admin` is on.
- Rename and drop update the list only after the engine call succeeds.
- Do not mention other database clients in commits, the plan, or code comments.
- Commits are English `type: lowercase subject` with `- ` bullet bodies. No `Co-authored-by` or other trailers. Create them with `git commit-tree` and `git reset --hard`, not `git commit`.
- Frontend tests: `cd web && npm test`. Typecheck: `cd web && npx tsc -p tsconfig.app.json --noEmit`. No Rust changes.
- The document item stores `sql` for a query, which the spec's open target carries and the item list must keep so the pane can mount. The state also stores `nextId`, starting at 1, so a rename does not change a document id and remount the pane. Ids are `d1`, `d2`, and so on.

---

## File structure

- `web/src/lib/documents.ts` — pure list. The only place identity, focus, close, rename, and drop are decided.
- `web/test/lib/documents.test.ts` — the tests named in the spec.
- `web/src/contexts/DocumentsContext.tsx` — holds one list per connection and the query dirty set.
- `web/src/components/workspace/DocumentTabBar.tsx` — the bar, the `+`, the Activity/Roles menu, the overflow menu.
- `web/src/components/workspace/DocumentHost.tsx` — mounts every document, hides the inactive ones, shows the empty state.
- `web/src/components/workspace/QueryPane.tsx` — one query. Moved out of the pill editor.
- `web/src/pages/DatabaseLayout.tsx` — sidebar without the three links, hosts the bar and the documents.
- `web/src/App.tsx` — `/db` has no child routes.
- `web/src/pages/TableView.tsx`, `TableStructure.tsx`, `FunctionView.tsx`, `ObjectView.tsx` — identity comes from props.
- `web/src/components/TableViewHeader.tsx` — the Data | Structure control is removed.
- `web/src/components/SchemaTree.tsx` — clicks and context menus call `openDocument`. Highlight follows the front document.
- `web/src/lib/openQuery.ts` — unchanged. The provider binds it to "open a new query document".

---

### Task 1: Document list

**Files:**
- Create: `web/src/lib/documents.ts`
- Test: `web/test/lib/documents.test.ts`

**Interfaces:**
- Consumes: `TableFilter` from `web/src/types/query.ts` (`{ column, operator, value }`).
- Produces: `DocumentsState`, `Document`, `OpenTarget`, `emptyDocuments`, `openDocument`, `closeDocument`, `focusDocument`, `renameQuery`, `renameRelation`, `renameSchema`, `dropRelation`, `dropSchema`, `seedQuery`.

- [ ] **Step 1: Write the failing test**

Create `web/test/lib/documents.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  closeDocument,
  dropRelation,
  dropSchema,
  emptyDocuments,
  openDocument,
  renameRelation,
  seedQuery,
} from "../../src/lib/documents.ts";

const filter = { column: "id", operator: "=", value: "1" };

test("opening the same table focuses its data tab, and structure is a second tab", () => {
  const once = openDocument(emptyDocuments(), { kind: "data", schema: "public", name: "comments", icon: "table" });
  const twice = openDocument(once, { kind: "data", schema: "public", name: "comments", icon: "table" });
  assert.equal(twice.items.length, 1);
  assert.equal(twice.activeId, once.activeId);
  const withStructure = openDocument(twice, { kind: "structure", schema: "public", name: "comments" });
  assert.equal(withStructure.items.length, 2);
  assert.equal(withStructure.items[1].kind, "structure");
  assert.equal(withStructure.activeId, withStructure.items[1].id);
});

test("a new query always adds a tab, and a saved query that is open is focused", () => {
  const first = openDocument(emptyDocuments(), { kind: "query", sql: "select 1" });
  const second = openDocument(first, { kind: "query", sql: "select 2" });
  assert.deepEqual(second.items.map((d) => d.title), ["Query 1", "Query 2"]);
  assert.equal(second.items[0].sql, "select 1");
  const saved = openDocument(second, { kind: "query", sql: "select 3", savedId: "s1", title: "Daily" });
  const again = openDocument(saved, { kind: "query", sql: "select 9", savedId: "s1", title: "Daily" });
  assert.equal(again.items.length, 3);
  assert.equal(again.activeId, saved.activeId);
  assert.equal(again.items.find((d) => d.savedId === "s1")?.sql, "select 3");
});

test("activity and roles open at most once", () => {
  const activity = openDocument(openDocument(emptyDocuments(), { kind: "activity" }), { kind: "activity" });
  const roles = openDocument(openDocument(activity, { kind: "roles" }), { kind: "roles" });
  assert.deepEqual(roles.items.map((d) => d.kind), ["activity", "roles"]);
});

test("closing the front tab focuses the right neighbor, and the last tab leaves the list empty", () => {
  let state = emptyDocuments();
  state = openDocument(state, { kind: "query" });
  state = openDocument(state, { kind: "query" });
  state = openDocument(state, { kind: "query" });
  const [a, b, c] = state.items;
  const closedB = closeDocument({ ...state, activeId: b.id }, b.id);
  assert.deepEqual(closedB.items.map((d) => d.id), [a.id, c.id]);
  assert.equal(closedB.activeId, c.id);
  const closedLast = closeDocument(closedB, c.id);
  assert.equal(closedLast.activeId, a.id);
  const empty = closeDocument(closedLast, a.id);
  assert.deepEqual(empty.items, []);
  assert.equal(empty.activeId, null);
  assert.equal(closeDocument(empty, "missing"), empty);
});

test("rename and drop update relation tabs, and dropping a schema removes its documents", () => {
  let state = openDocument(emptyDocuments(), { kind: "data", schema: "public", name: "comments", icon: "table" });
  state = openDocument(state, { kind: "structure", schema: "public", name: "comments" });
  state = openDocument(state, { kind: "index", schema: "public", name: "comments_pkey" });
  state = openDocument(state, { kind: "query" });
  const renamed = renameRelation(state, "public", "comments", "notes");
  assert.deepEqual(
    renamed.items.filter((d) => d.kind === "data" || d.kind === "structure").map((d) => d.name),
    ["notes", "notes"],
  );
  const dropped = dropRelation(renamed, "public", "notes");
  assert.deepEqual(dropped.items.map((d) => d.kind), ["index", "query"]);
  const schema = dropSchema(dropped, "public");
  assert.deepEqual(schema.items.map((d) => d.kind), ["query"]);
});

test("a foreign-key open records a new filter request and a plain focus does not", () => {
  const opened = openDocument(emptyDocuments(), {
    kind: "data", schema: "public", name: "comments", icon: "table", filters: [filter],
  });
  assert.deepEqual(opened.items[0].filterRequest, { id: 1, filters: [filter] });
  const again = openDocument(opened, {
    kind: "data", schema: "public", name: "comments", icon: "table", filters: [{ ...filter, value: "2" }],
  });
  assert.equal(again.items.length, 1);
  assert.deepEqual(again.items[0].filterRequest, { id: 2, filters: [{ ...filter, value: "2" }] });
  const plain = openDocument(again, { kind: "data", schema: "public", name: "comments", icon: "table" });
  assert.equal(plain.items[0].filterRequest?.id, 2);
});

test("seedQuery opens Query 1 only when the list is empty", () => {
  const seeded = seedQuery(emptyDocuments());
  assert.equal(seeded.items[0].title, "Query 1");
  assert.equal(seeded.items[0].sql, "SELECT 1;");
  assert.equal(seedQuery(seeded), seeded);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && node --experimental-strip-types --no-warnings --test test/lib/documents.test.ts`

Expected: FAIL, cannot find `web/src/lib/documents.ts`.

- [ ] **Step 3: Write the module**

Create `web/src/lib/documents.ts`:

```ts
import type { TableFilter } from "../types/query";

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
  | "trigger";

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
    };

export function emptyDocuments(): DocumentsState {
  return { items: [], activeId: null, nextQuery: 1, nextId: 1 };
}

function sameIdentity(doc: Document, target: Exclude<OpenTarget, { kind: "query" }>): boolean {
  if (doc.kind !== target.kind) return false;
  if (target.kind === "activity" || target.kind === "roles") return true;
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
  if (target.kind !== "query") {
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
    target.kind === "query" ? target.title ?? `Query ${state.nextQuery}`
    : target.kind === "activity" ? "Activity"
    : target.kind === "roles" ? "Roles"
    : target.name;
  const doc: Document = { id, kind: target.kind, title };
  if (target.kind === "query") {
    doc.sql = target.sql ?? "";
    if (target.savedId) doc.savedId = target.savedId;
  } else if (target.kind !== "activity" && target.kind !== "roles") {
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
  const index = state.items.findIndex((doc) => doc.id === id);
  if (index < 0) return state;
  const items = state.items.filter((doc) => doc.id !== id);
  if (state.activeId !== id) return { ...state, items };
  const neighbor = items[index] ?? items[index - 1] ?? null;
  return { ...state, items, activeId: neighbor?.id ?? null };
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
    items: state.items.map((doc) => (doc.schema === schema ? { ...doc, schema: newName } : doc)),
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
```

`dropRelation` closes data and structure only. An index tab stays, which the drop test asserts.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && node --experimental-strip-types --no-warnings --test test/lib/documents.test.ts`

Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/lib/documents.ts web/test/lib/documents.test.ts
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
test: cover the workspace document list

- opening an existing object focuses its tab
- close, rename and drop follow the neighbor and identity rules
EOF
)")
git reset --hard "$COMMIT"
```

---

### Task 2: Documents provider

**Files:**
- Create: `web/src/contexts/DocumentsContext.tsx`
- Test: `web/test/lib/documents.test.ts` (still the list; this task adds no rule)

**Interfaces:**
- Consumes: every function from Task 1, plus `useTab().tabId` and `bindOpenQuery` / `requestOpenQuery` from `web/src/lib/openQuery.ts`.
- Produces: `DocumentsProvider`, `useDocuments()` with `{ documents, openDocument, focusDocument, closeDocument, renameQuery, renameRelation, renameSchema, dropRelation, dropSchema, dirtyIds, setQueryDirty }`. `documents` is `DocumentsState`.

- [ ] **Step 1: Write the provider**

Create `web/src/contexts/DocumentsContext.tsx`. The provider calls `seedQuery(emptyDocuments())` as the initial state, so a connection mounts with Query 1. It binds `requestOpenQuery` for this connection tab to `openDocument({ kind: "query", sql })`.

```tsx
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { bindOpenQuery } from "../lib/openQuery";
import {
  closeDocument as closeIn,
  dropRelation as dropRelationIn,
  dropSchema as dropSchemaIn,
  emptyDocuments,
  focusDocument as focusIn,
  openDocument as openIn,
  renameQuery as renameQueryIn,
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
  renameQuery: (id: string, title: string) => void;
  renameRelation: (schema: string, name: string, newName: string) => void;
  renameSchema: (schema: string, newName: string) => void;
  dropRelation: (schema: string, name: string) => void;
  dropSchema: (schema: string) => void;
  dirtyIds: ReadonlySet<string>;
  setQueryDirty: (id: string, dirty: boolean) => void;
}

const DocumentsContext = createContext<DocumentsValue | null>(null);

export function DocumentsProvider({ children }: { children: ReactNode }) {
  const { tabId } = useTab();
  const [documents, setDocuments] = useState(() => seedQuery(emptyDocuments()));
  const [dirtyIds, setDirtyIds] = useState<ReadonlySet<string>>(() => new Set());

  const openDocument = useCallback((target: OpenTarget) => setDocuments((state) => openIn(state, target)), []);
  const focusDocument = useCallback((id: string) => setDocuments((state) => focusIn(state, id)), []);
  const closeDocument = useCallback((id: string) => {
    setDocuments((state) => closeIn(state, id));
    setDirtyIds((prev) => {
      if (!prev.has(id)) return prev;
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }, []);
  const renameQuery = useCallback((id: string, title: string) => setDocuments((state) => renameQueryIn(state, id, title)), []);
  const renameRelation = useCallback(
    (schema: string, name: string, newName: string) => setDocuments((state) => renameRelationIn(state, schema, name, newName)),
    [],
  );
  const renameSchema = useCallback((schema: string, newName: string) => setDocuments((state) => renameSchemaIn(state, schema, newName)), []);
  const dropRelation = useCallback((schema: string, name: string) => setDocuments((state) => dropRelationIn(state, schema, name)), []);
  const dropSchema = useCallback((schema: string) => setDocuments((state) => dropSchemaIn(state, schema)), []);
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
      documents, openDocument, focusDocument, closeDocument, renameQuery, renameRelation, renameSchema,
      dropRelation, dropSchema, dirtyIds, setQueryDirty,
    }),
    [documents, openDocument, focusDocument, closeDocument, renameQuery, renameRelation, renameSchema, dropRelation, dropSchema, dirtyIds, setQueryDirty],
  );

  return <DocumentsContext.Provider value={value}>{children}</DocumentsContext.Provider>;
}

export function useDocuments() {
  const ctx = useContext(DocumentsContext);
  if (!ctx) throw new Error("useDocuments must be used within DocumentsProvider");
  return ctx;
}
```

Do not mount this provider in the layout yet. Task 5 does that. The file only has to typecheck.

- [ ] **Step 2: Typecheck**

Run: `cd web && npx tsc -p tsconfig.app.json --noEmit`

Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/contexts/DocumentsContext.tsx
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
improve: hold document tabs for one connection

- seed a query when the connection mounts
- deliver open-sql requests as new query documents
EOF
)")
git reset --hard "$COMMIT"
```

---

### Task 3: Document tab bar

**Files:**
- Create: `web/src/components/workspace/DocumentTabBar.tsx`

**Interfaces:**
- Consumes: `Document` and `DataIcon` from Task 1. `useDocuments()` from Task 2. `useCapabilities().admin`.
- Produces: `DocumentTabBar`, rendered later by `DatabaseLayout`.

- [ ] **Step 1: Write the bar**

Create `web/src/components/workspace/DocumentTabBar.tsx`.

The bar is one row, `h-10`, lighter than the title-bar tabs: inactive text is `text-muted-foreground`, the active tab is `bg-accent font-medium text-foreground`, same weight as today's query pills.

Icons, from `lucide-react`, size 14:

| Document | Icon |
| --- | --- |
| query | `Terminal` |
| data `table` | `Table2` |
| data `view` | `Eye` |
| data `matview` | `Layers` |
| data `partition` | `Split` |
| structure | `Columns3` |
| activity | `Activity` |
| roles | `Users` |
| function, procedure | `Braces` |
| sequence | `ListOrdered` |
| type | `Shapes` |
| index | `ListTree` |
| trigger | `Zap` |

Each tab is a `role="tab"` button. Its label is `title`. When `schema` is set, render it after the title as `<span className="text-muted-foreground"> {schema}</span>` inside the button, smaller (`text-[11px]`). A query with `dirtyIds.has(id)` shows `<span className="size-1.5 rounded-full bg-primary" aria-label="Unsaved changes" />`. The close control is a `<span role="button">` with `X` size 12, `aria-label={`Close ${title}`}`, `onClick` stopPropagation then `closeDocument(id)`. It is shown even when one tab is open.

Double-click on a query tab swaps that tab's label for an input (`aria-label="Tab name"`, `defaultValue={title}`). Enter and blur call `renameQuery(id, value)`. Escape returns to the label without renaming. Other kinds do not rename.

`+` is a button `aria-label="New query"` calling `openDocument({ kind: "query" })`.

When `capabilities.admin`, a button `aria-label="Open a server screen"` opens a `Popover` with two items, Activity and Roles. Each calls `openDocument({ kind: "activity" })` or `{ kind: "roles" })` and closes the popover.

A button `aria-label="Show open tabs"` opens a `Popover` listing every document title. Choosing one calls `focusDocument(id)`.

The tab row is `overflow-x-auto`. The `+`, the server menu, and the open-tabs menu stay `shrink-0` outside the scroller.

Use `Popover` from `web/src/components/ui/Popover.tsx` (`open`, `onClose`, `anchorRef`, `children`).

- [ ] **Step 2: Typecheck**

Run: `cd web && npx tsc -p tsconfig.app.json --noEmit`

Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/components/workspace/DocumentTabBar.tsx
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
improve: add the document tab bar

- show an icon, the name and the schema, and always offer close
- add a new query, the server screens and the list of open tabs
EOF
)")
git reset --hard "$COMMIT"
```

---

### Task 4: One query pane

**Files:**
- Create: `web/src/components/workspace/QueryPane.tsx`
- Modify: `web/src/pages/QueryEditor.tsx` (delete the pill bar by making this file re-export the pane for the old route until Task 5 removes the route)
- Test: `web/test/lib/openQuery.test.ts` still passes. No new test; the list tests cover saved-query focus.

**Interfaces:**
- Consumes: `useDocuments().renameQuery` only when a saved query is renamed from the pane's existing save/rename path. `setQueryDirty(documentId, dirty)`.
- Produces: `QueryPane({ documentId, initialSql, savedId, title }: { documentId: string; initialSql: string; savedId?: string; title: string })`.

- [ ] **Step 1: Move the editor**

Copy `web/src/pages/QueryEditor.tsx` to `web/src/components/workspace/QueryPane.tsx`. Then remove the pill machinery. Delete `TabMeta`, `createQueryEditorState`, `repairQueryEditorState`, `tabMetas`, `activeId`, `switchTab`, `addTab`, `closeTab`, `renamingId`, and the pill JSX (the `role="tablist"` block around the comment "Query tabs as compact pills"). The component is one editor.

Props replace the active payload:

```tsx
export default function QueryPane({
  documentId,
  initialSql,
  savedId,
  title,
}: {
  documentId: string;
  initialSql: string;
  savedId?: string;
  title: string;
}) {
```

`SqlEditor` `initialValue` is `initialSql`. Result, error, plan, and view stay in `useState` inside this pane, so a hidden pane keeps them. Do not read them from `tabState`.

`useEffect` on `[documentId, setQueryDirty]` reports dirty the same way `setDirty` does today: compare the editor text with the saved query's sql when `savedId` is set, otherwise dirty when the text differs from `initialSql` after the first edit. Call `setQueryDirty(documentId, dirty)`.

Saving a query still calls `saveQuery`. On the first save, after `stored` comes back, call `renameQuery(documentId, stored.name)`. The saved id for that comparison lives in a `useState(savedId)` inside the pane. Updating the document's `savedId` is required so a later open focuses this tab. Add this function to `documents.ts` and `DocumentsContext` in this task:

```ts
export function setQuerySaved(state: DocumentsState, id: string, savedId: string, title: string): DocumentsState {
  return {
    ...state,
    items: state.items.map((doc) => (doc.id === id && doc.kind === "query" ? { ...doc, savedId, title } : doc)),
  };
}
```

Export it from the provider as `setQuerySaved(id, savedId, title)`. The pane calls it after a successful save. Add a test in `documents.test.ts`:

```ts
test("setQuerySaved stores the id used to focus that query later", () => {
  const opened = openDocument(emptyDocuments(), { kind: "query", sql: "select 1" });
  const saved = setQuerySaved(opened, opened.items[0].id, "s1", "Daily");
  const again = openDocument(saved, { kind: "query", sql: "other", savedId: "s1", title: "Daily" });
  assert.equal(again.items.length, 1);
  assert.equal(again.activeId, opened.activeId);
});
```

The history panel stays in the pane. `onOpenSaved` calls `openDocument({ kind: "query", sql: query.sql, savedId: query.id, title: query.name })` from `useDocuments()`, not a local tab. Opening a history row that is only SQL, with no saved id, calls `openDocument({ kind: "query", sql })`.

`bindOpenQuery` is not called here. The provider from Task 2 owns it.

Keyboard shortcuts in the pane (`run`, save) run only when `useTab().isActive` is true and the pane is the front document. The host passes `active: boolean`. Add `active: boolean` to the props and guard the key handler with it.

Until Task 5, `web/src/pages/QueryEditor.tsx` becomes a wrapper so the current route still compiles:

```tsx
import QueryPane from "../components/workspace/QueryPane";

export default function QueryEditor() {
  return <QueryPane documentId="route-query" initialSql={"SELECT 1;"} title="Query 1" active />;
}
```

That wrapper is deleted in Task 5. It does not call `useDocuments`, so the route works before the provider is mounted.

- [ ] **Step 2: Run tests and typecheck**

Run: `cd web && npm test && npx tsc -p tsconfig.app.json --noEmit`

Expected: the document tests pass, including `setQuerySaved`. Typecheck exits 0.

- [ ] **Step 3: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/lib/documents.ts web/src/contexts/DocumentsContext.tsx web/test/lib/documents.test.ts web/src/components/workspace/QueryPane.tsx web/src/pages/QueryEditor.tsx
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
improve: make a query one document

- the editor no longer keeps its own row of pills
- saving stores the id that a later open focuses
EOF
)")
git reset --hard "$COMMIT"
```

---

### Task 5: Workspace shell

**Files:**
- Create: `web/src/components/workspace/DocumentHost.tsx`
- Modify: `web/src/App.tsx` (delete the child routes under `/db`)
- Modify: `web/src/pages/DatabaseLayout.tsx`
- Modify: `web/src/pages/TableView.tsx`, `web/src/pages/TableStructure.tsx`, `web/src/pages/FunctionView.tsx`, `web/src/pages/ObjectView.tsx`
- Modify: `web/src/components/TableViewHeader.tsx`
- Modify: `web/src/components/SchemaTree.tsx`
- Delete the route wrapper body of `web/src/pages/QueryEditor.tsx` (the file can be removed if nothing imports it)

**Interfaces:**
- Consumes: `useDocuments()`, `DocumentTabBar`, `QueryPane`, the page components with the props below.
- Produces: a connected workspace whose main area is the bar plus the host. The tree opens documents.

- [ ] **Step 1: Give the pages props**

`TableView` takes `{ schema, name, filterRequest, active }` and no longer calls `useParams` or `useLocation`. Delete the `key={location.key}` wrapper. `initialFilters` is `filterRequest?.filters`. A `useEffect` depending on `filterRequest?.id` applies those filters through the existing `guard(tableData.applyFilters)` when the id is not the one the pane was created with. On first mount, `useTableData` already received them, so the effect skips the id it saw on mount (`useRef` of the initial id).

`TableStructure` takes `{ schema, name }`. Remove both `NavLink`s in its header (the Data | Structure pair). `TableLink` no longer navigates. It calls `openDocument({ kind: "data", schema, name: table, icon: "table" })`.

`FunctionView` takes `{ schema, name, detail }`. `detail` replaces `searchParams.get("args")`.

`ObjectView` takes `{ schema, name, kind }` and stops calling `useParams`.

`TableViewHeader` loses the `NavLink` pair and the imports of `NavLink`, `segmentedItemClass`, and `segmentedListClass`. Leave the divider out when it would sit at the end with nothing after it.

- [ ] **Step 2: Render the host**

Create `web/src/components/workspace/DocumentHost.tsx`:

```tsx
import { useDocuments } from "../../contexts/DocumentsContext";
import { useTab } from "../../contexts/workspace";
import QueryPane from "./QueryPane";
import TableView from "../../pages/TableView";
import TableStructure from "../../pages/TableStructure";
import FunctionView from "../../pages/FunctionView";
import ObjectView from "../../pages/ObjectView";
import ActivityView from "../../pages/ActivityView";
import RolesView from "../../pages/RolesView";
import Button from "../ui/Button";

export default function DocumentHost() {
  const { documents, openDocument } = useDocuments();
  const { isActive } = useTab();
  if (documents.items.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground">
        <p>Nothing is open.</p>
        <Button variant="outline" size="sm" onClick={() => openDocument({ kind: "query" })}>New query</Button>
      </div>
    );
  }
  return (
    <>
      {documents.items.map((doc) => {
        const front = doc.id === documents.activeId;
        return (
          <div key={doc.id} className={front ? "h-full min-h-0" : "hidden"} hidden={!front} inert={!front ? true : undefined}>
            {doc.kind === "query" && (
              <QueryPane documentId={doc.id} initialSql={doc.sql ?? ""} savedId={doc.savedId} title={doc.title} active={front && isActive} />
            )}
            {doc.kind === "data" && doc.schema && doc.name && (
              <TableView schema={doc.schema} name={doc.name} filterRequest={doc.filterRequest} active={front && isActive} />
            )}
            {doc.kind === "structure" && doc.schema && doc.name && <TableStructure schema={doc.schema} name={doc.name} />}
            {(doc.kind === "function" || doc.kind === "procedure") && doc.schema && doc.name && (
              <FunctionView schema={doc.schema} name={doc.name} detail={doc.detail ?? ""} />
            )}
            {(doc.kind === "sequence" || doc.kind === "type" || doc.kind === "index" || doc.kind === "trigger") && doc.schema && doc.name && (
              <ObjectView schema={doc.schema} name={doc.name} kind={doc.kind} />
            )}
            {doc.kind === "activity" && <ActivityView />}
            {doc.kind === "roles" && <RolesView />}
          </div>
        );
      })}
    </>
  );
}
```

`inert` expects a boolean. Passing `true` or omitting it matches the React type. Do not pass `inert={false}`.

In `TableView`, shortcuts that today check `useTab().isActive` check the `active` prop instead.

- [ ] **Step 3: Swap the layout and the routes**

In `web/src/App.tsx`, the `/db` route renders `DatabaseLayout` and has no child routes. Delete the imports of `QueryEditor`, `TableView`, `TableStructure`, `FunctionView`, `ObjectView`, `ActivityView`, and `RolesView` from `App.tsx`.

In `DatabaseLayout`, wrap the main column with `DocumentsProvider` inside the connected branch (the provider uses `useTab`, which already wraps the pane). Remove the Query, Activity, and Roles `NavLink`s and their icon imports if nothing else uses them. Remove `unavailableRoute` and the `Navigate` that sent those routes to the query. The `<main>` renders `DocumentTabBar` above `DocumentHost`, in a column (`flex h-full min-h-0 flex-col`). The right-panel slot stays where it is.

Delete `web/src/pages/QueryEditor.tsx` if the wrapper is the whole file.

- [ ] **Step 4: Point the tree at documents**

In `SchemaTree`, replace `useNavigate` for opening objects. Keep `navigate` only where Task 6 still needs it. Clicks:

- A table, view, or matview row calls `openDocument({ kind: "data", schema, name, icon })` with icon `table`, `view`, or `matview`.
- A partition row uses icon `partition`.
- A function or procedure passes `kind`, `schema`, `name`, and `detail: object.detail`.
- A sequence, type, index, or trigger passes `kind`, `schema`, and `name`.

Context menu **Open data** uses the same data target. **Open structure** uses `{ kind: "structure", schema, name }`.

`isActive(schema, object)` is true when the front document is that object: same schema and name, and kind `data` or `structure` for a relation, or the same kind (and, for a function or procedure, the same `detail`) otherwise. A query, Activity, or Roles front tab matches nothing. Partition rows use the same check with kind `data`.

`onCreated` of `CreateTableDialog` calls `openDocument({ kind: "data", schema, name: table, icon: "table" })` instead of `navigate`.

- [ ] **Step 5: Typecheck and test**

Run: `cd web && npm test && npx tsc -p tsconfig.app.json --noEmit`

Expected: tests pass, typecheck exits 0. Grep `web/src` for `Data` next to a structure `NavLink` and expect no matches. Grep `DatabaseLayout.tsx` for `to="/db/query"` and expect no matches.

- [ ] **Step 6: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/App.tsx web/src/pages/DatabaseLayout.tsx web/src/pages/TableView.tsx web/src/pages/TableStructure.tsx web/src/pages/FunctionView.tsx web/src/pages/ObjectView.tsx web/src/components/TableViewHeader.tsx web/src/components/SchemaTree.tsx web/src/components/workspace/DocumentHost.tsx web/src/pages/QueryEditor.tsx
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
improve: open explorer objects as document tabs

- the sidebar keeps the tree and drops the page links
- a hidden document stays mounted and ignores the keyboard
EOF
)")
git reset --hard "$COMMIT"
```

If `QueryEditor.tsx` was deleted, `git add` of that path stages the deletion.

---

### Task 6: Links, SQL, rename, drop, and truncate

**Files:**
- Modify: `web/src/pages/TableView.tsx`
- Modify: `web/src/pages/TableStructure.tsx`
- Modify: `web/src/components/SchemaTree.tsx`
- Modify: `web/src/components/schema/CreateTableDialog.tsx`
- Modify: `web/src/components/structure/StructureChangeDialog.tsx`

**Interfaces:**
- Consumes: `openDocument`, `renameRelation`, `renameSchema`, `dropRelation`, `dropSchema` from `useDocuments()`.
- Produces: the behaviors in the spec section **Opening and updating** that Task 5 did not cover.

- [ ] **Step 1: Foreign-key links ask before replacing filters**

`foreignKeyLink` returns `{ schema: fk.schema, name: fk.table, filters }` and no longer returns `path`.

The two call sites keep the current table's discard guard around the open, which is how leaving a dirty table works today:

```ts
guard(() => openDocument({ kind: "data", schema: link.schema, name: link.name, icon: "table", filters: link.filters }))
```

Cancelling that guard does not open anything. The table you are editing stays in front.

When the target data pane is already mounted, `openDocument` focuses it and stores a new `filterRequest`. The effect from Task 5 sees the new id and applies the filters through that pane's own `guard(tableData.applyFilters)`. Cancelling that second guard leaves the target tab in front with its previous filters. The effect records the id it already handled, so cancelling does not ask again until the next link.

- [ ] **Step 2: Open SQL and the structure links**

`CreateTableDialog` and `StructureChangeDialog` today call `requestOpenQuery(tabId, sql)` and `navigate("/db/query")`. Leave `requestOpenQuery`. The provider already turns it into a new query document, and it does not run the SQL. Delete the `navigate("/db/query")` calls so the dialog does not depend on a route. The new query becomes the front tab because `openDocument` sets `activeId`.

`TableStructure`'s `TableLink` already opens a data tab from Task 5. Links that carry filters, if any exist there, use the same `openDocument` data target with `filters`. A link without filters does not pass `filters`.

- [ ] **Step 3: Rename, drop, and truncate**

In `SchemaTree.runAction`, after a successful rename, call `renameRelation(target.schema, target.name, options.newName)` instead of `navigate`. After a successful drop, call `dropRelation(target.schema, target.name)` instead of `navigate("/db/query")`. After a successful truncate, do not close or navigate. If a data document for that schema and name is open, refresh it: keep a `Map<string, () => void>` on the provider (`refreshData`) that `TableView` registers on mount with key `${schema}.${name}` and the guarded `tableData.refresh`. `SchemaTree` calls `refreshData(schema, name)` after truncate. Unregister on unmount.

In `runContainer`, after a successful schema rename, call `renameSchema(name, options.name)` instead of `navigate`. After a successful schema drop, call `dropSchema(name)` instead of `navigate`. Create schema does not open a document. Duplicate does not open a document and does not call `openDocument`.

Remove `isCurrent` and `inSchema` if nothing else uses them.

- [ ] **Step 4: Typecheck and test**

Run: `cd web && npm test && npx tsc -p tsconfig.app.json --noEmit`

Expected: tests pass, typecheck exits 0. Grep `web/src` for `navigate("/db/query")` and expect no matches.

- [ ] **Step 5: Commit**

```bash
cd /Users/coelhomarcus/Developer/dbunny
git add web/src/pages/TableView.tsx web/src/pages/TableStructure.tsx web/src/components/SchemaTree.tsx web/src/components/schema/CreateTableDialog.tsx web/src/components/structure/StructureChangeDialog.tsx web/src/contexts/DocumentsContext.tsx
TREE=$(git write-tree)
COMMIT=$(git commit-tree "$TREE" -p HEAD -m "$(cat <<'EOF'
improve: keep document tabs in step with schema changes

- a foreign key opens the table with the link's filters
- rename and drop update the open tabs only after the engine accepts
EOF
)")
git reset --hard "$COMMIT"
```

---

## Self-review

Spec coverage:

- Two tab levels, switcher opens a connection: unchanged code, stated in Task 5's layout (switcher is not modified).
- Kinds, identity, query titles, saved query, Activity/Roles once: Task 1.
- Live hidden panes, inert, routes reduced to connect vs workspace: Task 5.
- Tab bar icons, schema suffix, dirty dot, rename, `+`, menus, empty state, Data | Structure removed: Tasks 3 and 5.
- Sidebar tree and highlight: Task 5.
- Foreign key, open SQL, history, rename, drop, truncate, duplicate: Tasks 4, 5, and 6.
- No confirm, connection shortcut untouched, load errors stay in the existing pages: no new close dialog; TitleBar is not modified; page error UI stays.
- Tests listed in the spec: Task 1, plus `setQuerySaved` in Task 4.

`nextId` and `sql` are the two fields added beyond the spec's item list. Both are required by behaviors the spec already states (stable panes across rename, and the SQL a new query opens with).
