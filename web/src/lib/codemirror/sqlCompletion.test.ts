import test from "node:test";
import assert from "node:assert/strict";
import { quoteIdentForDialect } from "./sqlCompletion.ts";

test("sqlite completion quotes reserved column names", () => {
  assert.equal(quoteIdentForDialect("index", "sqlite"), '"index"');
  assert.equal(quoteIdentForDialect("values", "sqlite"), '"values"');
  assert.equal(quoteIdentForDialect("title", "sqlite"), "title");
});

test("both dialects quote clearly reserved identifiers", () => {
  assert.equal(quoteIdentForDialect("user", "postgres"), '"user"');
  assert.equal(quoteIdentForDialect("select", "sqlite"), '"select"');
});
