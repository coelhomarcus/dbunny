import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addUserMessage,
  applyUpdate,
  attachApproval,
  emptyConversation,
  finishTurn,
  parseToolOutput,
  resolveApproval,
  toolLabel,
  type Conversation,
  type ToolItem,
} from "../../src/lib/assistantConversation.ts";

const chunk = (kind: string, text: string) => ({ sessionUpdate: kind, content: { type: "text", text } });
const call = (id: string, tool: string, input: Record<string, unknown>, status = "pending") => ({
  sessionUpdate: "tool_call",
  toolCallId: id,
  status,
  rawInput: input,
  _meta: { claudeCode: { toolName: `mcp__dbunny__${tool}` } },
});
const approval = (id: string, sql: string) => ({ id, conversationId: "c1", tool: "run_query", sql, analyze: false });

function play(updates: unknown[], from: Conversation = emptyConversation()): Conversation {
  return updates.reduce<Conversation>((state, update) => applyUpdate(state, update), from);
}

test("agent chunks join into one message, and a tool call splits them", () => {
  const joined = play([chunk("agent_message_chunk", "Hel"), chunk("agent_message_chunk", "lo")]);
  assert.deepEqual(joined.items, [{ kind: "agent", text: "Hello" }]);
  const split = play([chunk("agent_message_chunk", "a"), call("t1", "list_tables", {}), chunk("agent_message_chunk", "b")]);
  assert.deepEqual(split.items.map((item) => item.kind), ["agent", "tool", "agent"]);
});

test("thought chunks are their own item", () => {
  const state = play([chunk("agent_thought_chunk", "hmm"), chunk("agent_message_chunk", "ok")]);
  assert.deepEqual(state.items, [{ kind: "thought", text: "hmm" }, { kind: "agent", text: "ok" }]);
});

test("a tool call and its update complete one card", () => {
  const state = play([
    call("t1", "run_query", { sql: "SELECT 1" }),
    { sessionUpdate: "tool_call_update", toolCallId: "t1", status: "in_progress" },
    { sessionUpdate: "tool_call_update", toolCallId: "t1", status: "completed", content: [{ type: "content", content: { type: "text", text: '{"rowCount":1}' } }] },
  ]);
  assert.equal(state.items.length, 1);
  const item = state.items[0];
  assert.equal(item.kind, "tool");
  if (item.kind === "tool") {
    assert.equal(item.tool, "run_query");
    assert.deepEqual(item.input, { sql: "SELECT 1" });
    assert.equal(item.status, "completed");
    assert.equal(item.output, '{"rowCount":1}');
  }
});

test("an update for a call never announced makes its card", () => {
  const state = play([{ sessionUpdate: "tool_call_update", toolCallId: "late", status: "completed", _meta: { claudeCode: { toolName: "mcp__dbunny__list_tables" } } }]);
  assert.equal(state.items.length, 1);
  assert.equal(state.items[0].kind === "tool" && state.items[0].tool, "list_tables");
});

test("the tools of other servers keep their whole name", () => {
  const state = play([{ sessionUpdate: "tool_call", toolCallId: "x", status: "pending", _meta: { claudeCode: { toolName: "Bash" } } }]);
  assert.equal(state.items[0].kind === "tool" && state.items[0].tool, "Bash");
});

test("a replay rebuilds the user's and the agent's turns", () => {
  const state = play([chunk("user_message_chunk", "How many "), chunk("user_message_chunk", "rows?"), chunk("agent_message_chunk", "7")]);
  assert.deepEqual(state.items, [{ kind: "user", text: "How many rows?" }, { kind: "agent", text: "7" }]);
});

test("session_info_update sets the title", () => {
  assert.equal(play([{ sessionUpdate: "session_info_update", title: "Sales" }]).title, "Sales");
});

test("updates it does not know change nothing", () => {
  const before = emptyConversation();
  assert.equal(applyUpdate(before, { sessionUpdate: "plan", entries: [] }), before);
  assert.equal(applyUpdate(before, null), before);
  assert.equal(applyUpdate(before, "nonsense"), before);
});

test("an approval attaches to the running card with the same sql", () => {
  let state = play([call("t1", "run_query", { sql: "SELECT 1" })]);
  state = attachApproval(state, approval("a1", "SELECT 1"));
  assert.equal(state.items.length, 1);
  const item = state.items[0];
  assert.equal(item.kind === "tool" && item.approval?.id, "a1");
});

test("an approval with no matching card stands alone, and the card takes it when it arrives", () => {
  let state = attachApproval(emptyConversation(), approval("a1", "SELECT 1"));
  assert.deepEqual(state.items.map((item) => item.kind), ["approval"]);
  state = play([call("t1", "run_query", { sql: "SELECT 1" })], state);
  assert.deepEqual(state.items.map((item) => item.kind), ["tool"]);
  assert.equal(state.items[0].kind === "tool" && state.items[0].approval?.id, "a1");
});

test("resolving an approval clears it", () => {
  let state = attachApproval(play([call("t1", "run_query", { sql: "SELECT 1" })]), approval("a1", "SELECT 1"));
  state = resolveApproval(state, "a1");
  assert.equal(state.items[0].kind === "tool" && state.items[0].approval, undefined);
  const alone = resolveApproval(attachApproval(emptyConversation(), approval("a2", "SELECT 2")), "a2");
  assert.deepEqual(alone.items, []);
});

test("a turn starts with the user's message and ends running no more, with the error if it failed", () => {
  let state = addUserMessage(emptyConversation(), "hi");
  assert.equal(state.running, true);
  assert.deepEqual(state.items, [{ kind: "user", text: "hi" }]);
  state = finishTurn(state, "The assistant stopped");
  assert.equal(state.running, false);
  assert.deepEqual(state.items.at(-1), { kind: "error", text: "The assistant stopped" });
  assert.equal(finishTurn(addUserMessage(emptyConversation(), "x")).items.length, 1);
});

test("tool cards are labelled by what they did", () => {
  const tool = (name: string, input: Record<string, unknown> = {}): ToolItem => ({ kind: "tool", id: "t", tool: name, input, status: "completed" });
  assert.equal(toolLabel(tool("list_tables")), "Listed the tables");
  assert.equal(toolLabel(tool("list_tables", { schema: "audit" })), "Listed the tables of audit");
  assert.equal(toolLabel(tool("describe_table", { schema: "public", table: "orders" })), "Read the structure of public.orders");
  assert.equal(toolLabel(tool("relationships", { schema: "public" })), "Read the relationships of public");
  assert.equal(toolLabel(tool("run_query")), "Ran a query");
  assert.equal(toolLabel(tool("explain_query")), "Explained a query");
  assert.equal(toolLabel(tool("explain_query", { analyze: true })), "Explained a query and ran it");
  assert.equal(toolLabel(tool("Mystery")), "Mystery");
});

test("what the agent saw is read back from a tool's output", () => {
  const out = parseToolOutput(JSON.stringify({ columns: [{ name: "id", type: "int4" }], rows: [[1], [2]], rowCount: 2, truncated: true }));
  assert.deepEqual(out, { columns: [{ name: "id", type: "int4" }], rows: [[1], [2]], rowCount: 2, truncated: true, valuesHidden: false });
  const hidden = parseToolOutput(JSON.stringify({ columns: [{ name: "id", type: "int4" }], rowCount: 3, truncated: false, valuesHidden: true }));
  assert.equal(hidden?.valuesHidden, true);
  assert.deepEqual(hidden?.rows, []);
  assert.equal(parseToolOutput("The user declined to run this query."), null);
  assert.equal(parseToolOutput('{"schemas":[]}'), null);
  assert.equal(parseToolOutput(""), null);
});

test("an approval that arrives twice is shown once", () => {
  let state = play([call("t1", "run_query", { sql: "SELECT 1" })]);
  state = attachApproval(attachApproval(state, approval("a1", "SELECT 1")), approval("a1", "SELECT 1"));
  assert.equal(state.items.length, 1);
  const alone = attachApproval(attachApproval(emptyConversation(), approval("a2", "SELECT 2")), approval("a2", "SELECT 2"));
  assert.equal(alone.items.length, 1);
});

test("the text of a tool that failed comes without the code fence the agent puts around it", () => {
  const failed = (text: string) =>
    play([
      call("t1", "run_query", { sql: "SELECT 1" }),
      { sessionUpdate: "tool_call_update", toolCallId: "t1", status: "failed", content: [{ type: "content", content: { type: "text", text } }] },
    ]).items[0];
  const declined = "The user declined to run this query. Ask them how to continue.";
  assert.equal((failed("```\n" + declined + "\n```") as ToolItem).output, declined);
  // Text that is not wrapped in one stays as it is, and so does a fence that holds only part of it
  assert.equal((failed(declined) as ToolItem).output, declined);
  assert.equal((failed("Run this:\n```\nSELECT 1\n```") as ToolItem).output, "Run this:\n```\nSELECT 1\n```");
  // A result that is JSON is not touched either
  assert.equal((failed('{"rowCount":1}') as ToolItem).output, '{"rowCount":1}');
});
