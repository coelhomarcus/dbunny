import { test } from "node:test";
import assert from "node:assert/strict";
import {
  closeDocument,
  dropRelation,
  dropSchema,
  emptyDocuments,
  openDocument,
  renameRelation,
  renameSchema,
  seedQuery,
  setQuerySaved,
  updateCreateTable,
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

test("setQuerySaved stores the id used to focus that query later", () => {
  const opened = openDocument(emptyDocuments(), { kind: "query", sql: "select 1" });
  const saved = setQuerySaved(opened, opened.items[0].id, "s1", "Daily");
  const again = openDocument(saved, { kind: "query", sql: "other", savedId: "s1", title: "Daily" });
  assert.equal(again.items.length, 1);
  assert.equal(again.activeId, opened.activeId);
});

test("each new table page is its own tab, and the title follows the name", () => {
  const first = openDocument(emptyDocuments(), { kind: "create-table", schema: "public" });
  const second = openDocument(first, { kind: "create-table", schema: "public" });
  assert.equal(second.items.length, 2);
  assert.deepEqual(second.items.map((d) => d.title), ["New table", "New table"]);
  const titled = updateCreateTable(second, second.items[0].id, { title: "invoices", schema: "shop" });
  assert.equal(titled.items[0].title, "invoices");
  assert.equal(titled.items[0].schema, "shop");
  assert.equal(updateCreateTable(titled, titled.items[0].id, { title: "  " }).items[0].title, "New table");
  const renamed = renameSchema(titled, "public", "billing");
  assert.equal(renamed.items[1].schema, "billing");
  assert.equal(renamed.items[0].schema, "shop");
  const dropped = dropSchema(renamed, "billing");
  assert.deepEqual(dropped.items.map((d) => d.schema), ["shop"]);
});

test("seedQuery opens Query 1 only when the list is empty", () => {
  const seeded = seedQuery(emptyDocuments());
  assert.equal(seeded.items[0].title, "Query 1");
  assert.equal(seeded.items[0].sql, "SELECT 1;");
  assert.equal(seedQuery(seeded), seeded);
});
