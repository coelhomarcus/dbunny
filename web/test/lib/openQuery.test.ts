import { test } from "node:test";
import assert from "node:assert/strict";
import { bindOpenQuery, requestOpenQuery } from "../../src/lib/openQuery.ts";

test("requestOpenQuery delivers immediately when the editor is bound", () => {
  const opened: string[] = [];
  const unbind = bindOpenQuery("tab-a", (sql) => opened.push(sql));
  requestOpenQuery("tab-a", "SELECT 1");
  assert.deepEqual(opened, ["SELECT 1"]);
  unbind();
});

test("bindOpenQuery delivers a pending request from before mount", () => {
  requestOpenQuery("tab-b", "SELECT 2");
  const opened: string[] = [];
  bindOpenQuery("tab-b", (sql) => opened.push(sql));
  assert.deepEqual(opened, ["SELECT 2"]);
});

test("unbind stops later requests for that tab", () => {
  const opened: string[] = [];
  const unbind = bindOpenQuery("tab-c", (sql) => opened.push(sql));
  unbind();
  requestOpenQuery("tab-c", "SELECT 3");
  assert.equal(opened.length, 0);
  bindOpenQuery("tab-c", (sql) => opened.push(sql));
  assert.deepEqual(opened, ["SELECT 3"]);
});
