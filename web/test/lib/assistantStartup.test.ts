import { test } from "node:test";
import assert from "node:assert/strict";
import { startupHint, startupRows } from "../../src/lib/assistantStartup.ts";

test("the steps go in order, the ones before the current done and the ones after waiting", () => {
  const rows = startupRows("tools", "Codex");
  assert.deepEqual(
    rows.map((row) => [row.id, row.state]),
    [["shell", "done"], ["node", "done"], ["tools", "current"], ["agent", "pending"]],
  );
  assert.equal(rows[0].label, "Reading your shell settings to find Node.js");
  assert.equal(rows[1].label, "Checking Node.js");
  assert.equal(rows[2].label, "Starting DBunny's tool server");
  assert.equal(rows[3].label, "Starting Codex");
});

test("the first step has nothing done, and the last has all the others", () => {
  assert.deepEqual(startupRows("shell", "Claude Code").map((row) => row.state), ["current", "pending", "pending", "pending"]);
  assert.deepEqual(startupRows("agent", "Claude Code").map((row) => row.state), ["done", "done", "done", "current"]);
});

test("a custom agent is started by that name", () => {
  assert.equal(startupRows("agent", "your agent")[3].label, "Starting your agent");
});

test("the wait for the agent explains itself the longer it lasts", () => {
  assert.equal(startupHint("agent", 3, "codex"), null);
  assert.match(startupHint("agent", 10, "codex") ?? "", /first time.*download/i);
  assert.match(startupHint("agent", 120, "claude") ?? "", /still|slow/i);
  // The other steps are quick; no explanation unless they are not
  assert.equal(startupHint("tools", 3, "codex"), null);
  assert.match(startupHint("shell", 8, "codex") ?? "", /shell/i);
  // A custom command is not downloaded by DBunny
  assert.equal(startupHint("agent", 10, "custom"), null);
});
