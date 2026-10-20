import { test } from "node:test";
import assert from "node:assert/strict";
import { groupByDay } from "../../src/lib/assistantHistory.ts";

const at = (iso: string) => ({ acpSession: iso, title: iso, updatedAt: iso });
// 2026-10-20 at noon, local time
const now = new Date(2026, 9, 20, 12, 0, 0);
const local = (day: number, hour = 9) => new Date(2026, 9, day, hour, 0, 0).toISOString();

test("conversations are grouped by the day they were last used, the newest first", () => {
  const groups = groupByDay([at(local(18)), at(local(20, 8)), at(local(19, 23)), at(local(20, 11))], now);
  assert.deepEqual(groups.map((g) => g.label), ["Today", "Yesterday", "Oct 18"]);
  assert.deepEqual(groups[0].entries.map((e) => e.updatedAt), [local(20, 11), local(20, 8)]);
});

test("a conversation of another year says the year", () => {
  assert.equal(groupByDay([{ acpSession: "x", title: null, updatedAt: new Date(2025, 11, 3, 10).toISOString() }], now)[0].label, "Dec 3, 2025");
});

test("what has no date goes last, as earlier", () => {
  const groups = groupByDay([{ acpSession: "n", title: "n", updatedAt: null }, at(local(20, 9)), { acpSession: "b", title: "b", updatedAt: "not a date" }], now);
  assert.deepEqual(groups.map((g) => g.label), ["Today", "Earlier"]);
  assert.equal(groups[1].entries.length, 2);
});

test("with nothing there is nothing", () => {
  assert.deepEqual(groupByDay([], now), []);
});
