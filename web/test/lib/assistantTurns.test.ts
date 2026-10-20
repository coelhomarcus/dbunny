import { test } from "node:test";
import assert from "node:assert/strict";
import { applyUpdate, emptyConversation, type Conversation, type Item, type ToolItem } from "../../src/lib/assistantConversation.ts";
import { formatDuration, groupItems, stepsSummary, toolDescription } from "../../src/lib/assistantTurns.ts";

const text = (kind: string, value: string) => ({ sessionUpdate: kind, content: { type: "text", text: value } });
const call = (id: string, tool: string, input: Record<string, unknown> = {}) => ({
  sessionUpdate: "tool_call",
  toolCallId: id,
  status: "in_progress",
  rawInput: input,
  _meta: { claudeCode: { toolName: `mcp__dbunny__${tool}` } },
});
const done = (id: string, status: string, output = "") => ({
  sessionUpdate: "tool_call_update",
  toolCallId: id,
  status,
  content: output ? [{ type: "content", content: { type: "text", text: output } }] : undefined,
});
const tool = (over: Partial<ToolItem> = {}): ToolItem => ({ kind: "tool", id: "t", tool: "run_query", input: {}, status: "completed", ...over });
const user: Item = { kind: "user", text: "q" };

test("what happens live carries when it happened, and a replay carries nothing", () => {
  let live: Conversation = emptyConversation();
  live = applyUpdate(live, text("agent_thought_chunk", "hmm"), 1000);
  live = applyUpdate(live, call("a", "list_tables"), 2000);
  live = applyUpdate(live, done("a", "completed", "{}"), 3500);
  const [thought, listed] = live.items as [Item & { at?: number }, ToolItem];
  assert.equal(thought.at, 1000);
  assert.deepEqual([listed.at, listed.endedAt], [2000, 3500]);
  const replay = applyUpdate(applyUpdate(emptyConversation(), call("a", "list_tables")), done("a", "completed"));
  assert.deepEqual([(replay.items[0] as ToolItem).at, (replay.items[0] as ToolItem).endedAt], [undefined, undefined]);
});

test("a step that ends again keeps the time it first ended", () => {
  let state = applyUpdate(emptyConversation(), call("a", "list_tables"), 1000);
  state = applyUpdate(state, done("a", "completed"), 2000);
  state = applyUpdate(state, done("a", "completed"), 9000);
  assert.equal((state.items[0] as ToolItem).endedAt, 2000);
});

test("durations read as a person would say them", () => {
  assert.equal(formatDuration(400), "0.4s");
  assert.equal(formatDuration(1000), "1s");
  assert.equal(formatDuration(14200), "14s");
  assert.equal(formatDuration(65000), "1m 5s");
  assert.equal(formatDuration(120000), "2m");
});

test("a step says what it found, then how long it took", () => {
  const rows = JSON.stringify({ columns: [{ name: "n", type: "int" }], rows: [[1], [2], [3]], rowCount: 3, truncated: false });
  assert.equal(toolDescription(tool({ output: rows, at: 1000, endedAt: 1400 })), "3 rows · 0.4s");
  assert.equal(toolDescription(tool({ output: rows })), "3 rows");
  assert.equal(toolDescription(tool({ output: JSON.stringify({ columns: [], rowCount: 1, truncated: true }) })), "1 row (cut)");
  assert.equal(toolDescription(tool({ output: JSON.stringify({ columns: [{ name: "a", type: "t" }], rowCount: 5, valuesHidden: true }) })), "5 rows, values hidden");
  assert.equal(toolDescription(tool({ tool: "list_tables", output: JSON.stringify({ schemas: [{ name: "public", tables: [{}, {}] }, { name: "audit", tables: [{}] }] }) })), "3 tables");
  assert.equal(toolDescription(tool({ tool: "describe_table", output: JSON.stringify({ columns: [{}, {}, {}, {}] }) })), "4 columns");
  assert.equal(toolDescription(tool({ tool: "relationships", output: JSON.stringify({ relations: ["a", "b"] }) })), "2 relations");
  assert.equal(toolDescription(tool({ tool: "describe_table", output: JSON.stringify({ columns: [{}] }) })), "1 column");
  // Running, failed or unreadable: nothing to say yet
  assert.equal(toolDescription(tool({ status: "in_progress", output: undefined })), "");
  assert.equal(toolDescription(tool({ status: "failed", output: "boom", at: 1000, endedAt: 2000 })), "1s");
});

test("the steps of a turn are one block, with the texts between them standing apart", () => {
  const items: Item[] = [user, { kind: "thought", text: "t" }, tool({ id: "a" }), { kind: "agent", text: "narration" }, tool({ id: "b" }), tool({ id: "c" }), { kind: "agent", text: "answer" }];
  const blocks = groupItems(items, false);
  assert.deepEqual(blocks.map((b) => (b.kind === "steps" ? `steps:${b.items.length}` : b.item.kind)), ["user", "steps:2", "agent", "steps:2", "agent"]);
});

test("a block of steps is folded when it is over, and open while it goes on or waits for the person", () => {
  const finished = groupItems([user, tool({ id: "a" }), { kind: "agent", text: "answer" }], false);
  assert.equal(finished[1].kind === "steps" && finished[1].folded, true);
  // The turn is still running and these are its last steps
  const live = groupItems([user, tool({ id: "a" })], true);
  assert.equal(live[1].kind === "steps" && live[1].folded, false);
  // A step still running, or a query waiting, keeps its block open even when text came after
  const going = groupItems([user, tool({ id: "a", status: "in_progress" }), { kind: "agent", text: "x" }], false);
  assert.equal(going[1].kind === "steps" && going[1].folded, false);
  const waiting = groupItems([user, { kind: "approval", approval: { id: "p", conversationId: "c", tool: "run_query", sql: "SELECT 1", analyze: false } }, { kind: "agent", text: "x" }], false);
  assert.equal(waiting[1].kind === "steps" && waiting[1].folded, false);
  // An earlier block is over once text follows it, though the turn runs on
  const earlier = groupItems([user, tool({ id: "a" }), { kind: "agent", text: "narration" }, tool({ id: "b", status: "in_progress" })], true);
  assert.deepEqual(earlier.map((b) => (b.kind === "steps" ? b.folded : null)), [null, true, null, false]);
});

test("the summary counts the steps and says how long they took", () => {
  const steps: Item[] = [{ kind: "thought", text: "t", at: 1000 }, tool({ id: "a", at: 2000, endedAt: 5000 }), tool({ id: "b", at: 5000, endedAt: 15000 })];
  assert.equal(stepsSummary(steps, 16000), "Worked for 15s · 3 steps");
  assert.equal(stepsSummary([tool({ id: "a" })], undefined), "1 step");
  assert.equal(stepsSummary([{ kind: "thought", text: "t", at: 1000 }], 5000), "Thought for 4s");
  assert.equal(stepsSummary([{ kind: "thought", text: "t" }], undefined), "Thought");
});
