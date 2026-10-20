import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";

// The preferences live in the browser's local storage; these tests bring a small one
const store = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  },
});

const { conversationOptions, getConnectionPrefs, parseRowLimit, setConnectionPrefs } = await import("../../src/lib/assistantPrefs.ts");

beforeEach(() => store.clear());

test("a connection with nothing stored gets the defaults", () => {
  assert.deepEqual(getConnectionPrefs("c1"), { schemaOnly: false, instructions: "" });
  assert.deepEqual(getConnectionPrefs(null), { schemaOnly: false, instructions: "" });
});

test("preferences are kept per saved connection", () => {
  setConnectionPrefs("c1", { schemaOnly: true, instructions: "status 3 means cancelled" });
  setConnectionPrefs("c2", { schemaOnly: false, instructions: "other" });
  assert.deepEqual(getConnectionPrefs("c1"), { schemaOnly: true, instructions: "status 3 means cancelled" });
  assert.deepEqual(getConnectionPrefs("c2"), { schemaOnly: false, instructions: "other" });
});

test("a connection that was never saved stores nothing", () => {
  setConnectionPrefs(null, { schemaOnly: true, instructions: "x" });
  assert.equal(store.size, 0);
  assert.deepEqual(getConnectionPrefs(null), { schemaOnly: false, instructions: "" });
});

test("damaged storage falls back to the defaults", () => {
  store.set("dbunny:assistant:connections", "{not json");
  assert.deepEqual(getConnectionPrefs("c1"), { schemaOnly: false, instructions: "" });
  store.set("dbunny:assistant:connections", JSON.stringify({ c1: { schemaOnly: "yes", instructions: 4 } }));
  assert.deepEqual(getConnectionPrefs("c1"), { schemaOnly: false, instructions: "" });
});

test("a conversation's options combine the settings and the connection's preferences", () => {
  const options = conversationOptions({ assistantAskBeforeQueries: true, assistantRowLimit: 100 }, { schemaOnly: true, instructions: "hi" }, "en");
  assert.deepEqual(options, { askBeforeQueries: true, schemaOnly: true, rowLimit: 100, instructions: "hi", language: "en" });
});

test("the options carry the language of the app, so the agent answers in it", () => {
  const prefs = { schemaOnly: false, instructions: "" };
  assert.equal(conversationOptions({ assistantAskBeforeQueries: true, assistantRowLimit: 100 }, prefs, "pt-BR").language, "pt-BR");
});

test("the row limit stays between 10 and 1000", () => {
  const prefs = { schemaOnly: false, instructions: "" };
  assert.equal(conversationOptions({ assistantAskBeforeQueries: false, assistantRowLimit: 3 }, prefs, "en").rowLimit, 10);
  assert.equal(conversationOptions({ assistantAskBeforeQueries: false, assistantRowLimit: 5000 }, prefs, "en").rowLimit, 1000);
  assert.equal(conversationOptions({ assistantAskBeforeQueries: false, assistantRowLimit: Number.NaN }, prefs, "en").rowLimit, 100);
});

test("what is typed as the row limit becomes a number between 10 and 1000", () => {
  assert.equal(parseRowLimit("250"), 250);
  assert.equal(parseRowLimit(" 40 "), 40);
  assert.equal(parseRowLimit("12.6"), 13);
  assert.equal(parseRowLimit("5"), 10);
  assert.equal(parseRowLimit("5000"), 1000);
});

test("an empty or unreadable row limit is the default, not zero or NaN", () => {
  assert.equal(parseRowLimit(""), 100);
  assert.equal(parseRowLimit("   "), 100);
  assert.equal(parseRowLimit("abc"), 100);
  assert.equal(parseRowLimit("1e999"), 100);
});
