import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { clearFormatSqlCache, formatSqlCached } from "../../src/lib/formatSql.ts";

afterEach(() => clearFormatSqlCache());

test("formatSqlCached upper-cases keywords and reuses cache", async () => {
  const sql = "select 1";
  const a = await formatSqlCached(sql, "postgres");
  const b = await formatSqlCached(sql, "postgres");
  assert.equal(a, b);
  assert.match(a, /^SELECT/);
});

test("mysql sql is formatted as mysql", async () => {
  const out = await formatSqlCached("select `a`, b from `t` where `a` = 1", "mysql");
  assert.match(out, /^SELECT/);
  assert.match(out, /`a`/);
  assert.match(out, /FROM\s+`t`/);
  assert.doesNotMatch(out, /"a"/);
});
