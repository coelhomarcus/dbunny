import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { clearFormatSqlCache, formatSqlCached } from "./formatSql.ts";

afterEach(() => clearFormatSqlCache());

test("formatSqlCached upper-cases keywords and reuses cache", async () => {
  const sql = "select 1";
  const a = await formatSqlCached(sql, "postgres");
  const b = await formatSqlCached(sql, "postgres");
  assert.equal(a, b);
  assert.match(a, /^SELECT/);
});
