import { test } from "node:test";
import assert from "node:assert/strict";
import { stripContext } from "../../src/lib/assistantContext.ts";

test("the database context after the question is not shown", () => {
  assert.equal(stripContext("Quantos pedidos?\n\n<dbunny-context>\nThe database:\n- Engine: SQLite\n</dbunny-context>"), "Quantos pedidos?");
});

test("a message without the block is as it was", () => {
  assert.equal(stripContext("Quantos pedidos?"), "Quantos pedidos?");
  assert.equal(stripContext("  keep\n  spaces  "), "  keep\n  spaces  ");
});

test("a block that is still arriving is cut from where it opens", () => {
  assert.equal(stripContext("Q\n\n<dbunny-context>\nThe data"), "Q");
});

test("an opening cut in the middle of the stream is not an opening yet", () => {
  assert.equal(stripContext("Q\n\n<dbunny-con"), "Q\n\n<dbunny-con");
});

test("only the block goes when the question has several lines", () => {
  assert.equal(stripContext("line 1\nline 2\n\n<dbunny-context>\nx\n</dbunny-context>"), "line 1\nline 2");
});

test("two blocks are both removed", () => {
  assert.equal(stripContext("A\n\n<dbunny-context>\nx\n</dbunny-context>\n\nB\n\n<dbunny-context>\ny\n</dbunny-context>"), "A\n\nB");
});
