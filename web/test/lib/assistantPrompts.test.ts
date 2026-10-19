import { test } from "node:test";
import assert from "node:assert/strict";
import { errorTextOf, explainPrompt, fixPrompt, optimizePrompt } from "../../src/lib/assistantPrompts.ts";

const sql = "SELECT * FROM orders WHERE id = 1";
const fence = "```sql\n" + sql + "\n```";

test("the explain request names the engine and ends with the statement", () => {
  const text = explainPrompt({ engine: "PostgreSQL", sql });
  assert.ok(text.startsWith("Explain what this PostgreSQL query does, step by step, in plain words."));
  assert.ok(text.endsWith(fence), text);
});

test("the optimize request asks for the plan, and carries the one just run only when there is one", () => {
  const without = optimizePrompt({ engine: "MySQL", sql });
  assert.ok(without.startsWith("Analyze the performance of this MySQL query. Use explain_query to read its plan (ask before running it with analyze)"));
  assert.ok(!without.includes("The plan I just ran"));
  assert.ok(without.endsWith(fence));

  const withPlan = optimizePrompt({ engine: "MySQL", sql, plan: '[{"Plan":"Seq Scan"}]' });
  assert.ok(withPlan.includes('The plan I just ran:\n```\n[{"Plan":"Seq Scan"}]\n```'));
  assert.ok(withPlan.endsWith(fence));
});

test("the fix request carries the error and the statement that failed", () => {
  const text = fixPrompt({ engine: "SQLite", sql, error: 'no such column: "nme"' });
  assert.ok(text.startsWith('This SQLite query failed with:\n\nno such column: "nme"\n\n'));
  assert.ok(text.includes("suggest the corrected SQL in a sql block."));
  assert.ok(text.endsWith(fence));
});

test("a statement that holds a code fence is wrapped in a longer one", () => {
  const tricky = "SELECT '```' AS fence";
  const text = explainPrompt({ engine: "SQLite", sql: tricky });
  assert.ok(text.endsWith("````sql\n" + tricky + "\n````"), text);
});

test("an error of the console reads as its message, with the detail and the hint the server gave", () => {
  const raw = JSON.stringify({ message: 'column "nme" does not exist', severity: "ERROR", detail: null, hint: 'Perhaps you meant "name".', position: 8 });
  assert.equal(errorTextOf(raw), 'column "nme" does not exist\nHint: Perhaps you meant "name".');
  assert.equal(errorTextOf(JSON.stringify({ message: "boom", detail: "more" })), "boom\nDetail: more");
  assert.equal(errorTextOf("Query failed"), "Query failed");
  assert.equal(errorTextOf("{not json"), "{not json");
});
