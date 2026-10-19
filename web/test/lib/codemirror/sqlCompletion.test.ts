import test from "node:test";
import assert from "node:assert/strict";
import { MySQL, PostgreSQL, SQLite } from "@codemirror/lang-sql";
import { dialectFor, quoteIdentForDialect } from "../../../src/lib/codemirror/sqlCompletion.ts";

test("sqlite completion quotes reserved column names", () => {
  assert.equal(quoteIdentForDialect("index", "sqlite"), '"index"');
  assert.equal(quoteIdentForDialect("values", "sqlite"), '"values"');
  assert.equal(quoteIdentForDialect("title", "sqlite"), "title");
});

test("both dialects quote clearly reserved identifiers", () => {
  assert.equal(quoteIdentForDialect("user", "postgres"), '"user"');
  assert.equal(quoteIdentForDialect("select", "sqlite"), '"select"');
});

test("completion quotes mysql names with backticks", () => {
  assert.equal(quoteIdentForDialect("select", "mysql"), "`select`");
  assert.equal(quoteIdentForDialect("my col", "mysql"), "`my col`");
  assert.equal(quoteIdentForDialect("a`b", "mysql"), "`a``b`");
  assert.equal(quoteIdentForDialect("customers", "mysql"), "customers");
  // Double quotes are strings to MySQL, never names
  assert.doesNotMatch(quoteIdentForDialect("Order Items", "mysql"), /"/);
});

test("mysql completes with the mysql dialect", () => {
  assert.equal(dialectFor("mysql"), MySQL);
  assert.equal(dialectFor("sqlite"), SQLite);
  assert.equal(dialectFor("postgres"), PostgreSQL);
});
