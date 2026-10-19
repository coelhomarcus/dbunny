import assert from "node:assert/strict";
import { test } from "node:test";
import type { QueryColumn } from "@/types";
import { pendingChangesToSql, quoteIdentFor, rowsToInsertSql, sqlLiteralFor } from "../../src/lib/copyRows.ts";

const columns: QueryColumn[] = [
  { name: "id", dataType: "int" },
  { name: "note", dataType: "text" },
];

test("mysql copy uses backticks", () => {
  const sql = rowsToInsertSql("shop", "orders", columns, [[1, "hi"]], "mysql");
  assert.equal(sql, "INSERT INTO `shop`.`orders` (`id`, `note`) VALUES\n  (1, 'hi');");
  assert.equal(quoteIdentFor("mysql")("a`b"), "`a``b`");
});

test("postgres and sqlite keep double quotes", () => {
  const expected = 'INSERT INTO "shop"."orders" ("id", "note") VALUES\n  (1, \'hi\');';
  assert.equal(rowsToInsertSql("shop", "orders", columns, [[1, "hi"]]), expected);
  assert.equal(rowsToInsertSql("shop", "orders", columns, [[1, "hi"]], "postgres"), expected);
  assert.equal(rowsToInsertSql("shop", "orders", columns, [[1, "hi"]], "sqlite"), expected);
  assert.equal(quoteIdentFor("sqlite")('a"b'), '"a""b"');
});

test("mysql literals escape backslashes", () => {
  assert.equal(sqlLiteralFor("mysql")("a\\b'c"), "'a\\\\b''c'");
  // PostgreSQL reads a backslash as itself
  assert.equal(sqlLiteralFor("postgres")("a\\b'c"), "'a\\b''c'");
  assert.equal(sqlLiteralFor("sqlite")("a\\b'c"), "'a\\b''c'");
});

test("every dialect writes null, numbers and booleans the same way", () => {
  for (const dialect of ["postgres", "sqlite", "mysql"] as const) {
    const literal = sqlLiteralFor(dialect);
    assert.deepEqual([literal(null), literal(undefined), literal(5), literal(2n), literal(true), literal(false)], ["NULL", "NULL", "5", "2", "TRUE", "FALSE"]);
    assert.equal(literal({ a: 1 }), `'{"a":1}'`);
  }
});

test("pending edits are written in the dialect", () => {
  const changes = [{ key: { id: 1 }, updates: { note: "x\\y" } }];
  assert.equal(pendingChangesToSql("shop", "orders", changes, "mysql"), "UPDATE `shop`.`orders`\nSET `note` = 'x\\\\y'\nWHERE `id` = 1;");
  assert.equal(pendingChangesToSql("shop", "orders", changes), 'UPDATE "shop"."orders"\nSET "note" = \'x\\y\'\nWHERE "id" = 1;');
});
