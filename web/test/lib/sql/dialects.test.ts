import test from "node:test";
import assert from "node:assert/strict";
import { MariaSQL, MySQL, PostgreSQL, SQLite } from "@codemirror/lang-sql";
import { available, dialectOf, functionKey, languageFor } from "../../../src/lib/sql/dialects/index.ts";
import { en } from "../../../src/i18n/locales/en.ts";
import { ptBR } from "../../../src/i18n/locales/pt-BR.ts";
import type { Engine } from "../../../src/types/connection.ts";

const ENGINES: Engine[] = ["postgres", "mysql", "mariadb", "sqlite"];
const CATEGORIES = ["text", "numeric", "datetime", "json", "aggregate", "window", "conditional", "system"];

const names = (engine: Engine) => new Set(dialectOf(engine).functions.map((f) => f.name.toLowerCase()));
const phrases = (engine: Engine) => new Set(dialectOf(engine).phrases.map((p) => p.label));

test("every engine has a dialect", () => {
  for (const engine of ENGINES) assert.equal(dialectOf(engine).engine, engine);
  assert.equal(languageFor("postgres"), PostgreSQL);
  assert.equal(languageFor("mysql"), MySQL);
  assert.equal(languageFor("mariadb"), MariaSQL);
  assert.equal(languageFor("sqlite"), SQLite);
  assert.equal(dialectOf("mariadb").language, MariaSQL);
});

test("quoting follows each dialect", () => {
  assert.equal(dialectOf("mysql").quoteIdent("order"), "`order`");
  assert.equal(dialectOf("mysql").quoteIdent("a`b"), "`a``b`");
  assert.equal(dialectOf("postgres").quoteIdent("Order"), '"Order"');
  assert.equal(dialectOf("postgres").quoteIdent("user"), '"user"');
  assert.equal(dialectOf("mariadb").quoteIdent("nome com espaço"), "`nome com espaço`");
  assert.equal(dialectOf("sqlite").quoteIdent("index"), '"index"');
  for (const engine of ENGINES) assert.equal(dialectOf(engine).quoteIdent("customers"), "customers");
  assert.equal(dialectOf("mysql").identifierQuote, "`");
  assert.equal(dialectOf("sqlite").identifierQuote, '"');
  assert.ok(dialectOf("mysql").lineComments.includes("#"));
  assert.ok(!dialectOf("postgres").lineComments.includes("#"));
});

test("phrases never leak between dialects", () => {
  for (const engine of ["mysql", "mariadb"] as const) {
    for (const phrase of ["ON CONFLICT", "NULLS FIRST", "DO NOTHING", "FULL JOIN", "INSERT OR REPLACE"]) {
      assert.ok(!phrases(engine).has(phrase), `${phrase} in ${engine}`);
    }
    assert.ok(phrases(engine).has("ON DUPLICATE KEY UPDATE"));
  }
  for (const engine of ["postgres", "sqlite"] as const) {
    assert.ok(!phrases(engine).has("ON DUPLICATE KEY UPDATE"), engine);
    assert.ok(phrases(engine).has("ON CONFLICT"), engine);
  }
  assert.ok(!phrases("postgres").has("INSERT OR REPLACE"));
  assert.ok(phrases("sqlite").has("INSERT OR REPLACE"));
});

test("catalogs are exclusive where the research says so", () => {
  const only: Record<string, Engine[]> = {
    date_trunc: ["postgres"], generate_series: ["postgres"], jsonb_set: ["postgres"], jsonb_build_object: ["postgres"],
    to_char: ["postgres"], age: ["postgres"], gen_random_uuid: ["postgres"], array_agg: ["postgres"], pg_typeof: ["postgres"],
    uuid_to_bin: ["mysql"], bin_to_uuid: ["mysql"], regexp_like: ["mysql"], json_pretty: ["mysql"], statement_digest: ["mysql"],
    nvl: ["mariadb"], json_detailed: ["mariadb"],
    date_format: ["mysql", "mariadb"], str_to_date: ["mysql", "mariadb"], timestampdiff: ["mysql", "mariadb"],
    datediff: ["mysql", "mariadb"], curdate: ["mysql", "mariadb"], last_insert_id: ["mysql", "mariadb"],
    strftime: ["sqlite"], julianday: ["sqlite"], unixepoch: ["sqlite"], last_insert_rowid: ["sqlite"],
    sqlite_version: ["sqlite"], total: ["sqlite"], iif: ["sqlite"], json_group_array: ["sqlite"],
  };
  for (const [name, owners] of Object.entries(only)) {
    for (const engine of ENGINES) {
      assert.equal(names(engine).has(name), owners.includes(engine), `${name} in ${engine}`);
    }
  }
  // MySQL has JSON_VALUE only from 8.0.21, so its catalog leaves it to MariaDB
  assert.ok(!names("mysql").has("json_value"));
});

test("catalog entries are complete", () => {
  for (const engine of ENGINES) {
    const { functions } = dialectOf(engine);
    assert.ok(functions.length >= 70, `${engine} has ${functions.length}`);
    assert.equal(new Set(functions.map((f) => f.name)).size, functions.length, `${engine} repeats a name`);
    assert.equal(new Set(functions.map((f) => functionKey(f.name))).size, functions.length, `${engine} repeats a key`);
    for (const f of functions) {
      assert.ok(f.name && f.signature, `${engine} ${f.name}`);
      assert.ok(CATEGORIES.includes(f.category), `${engine} ${f.name} ${f.category}`);
      assert.ok(f.example.trim().length > 0, `${engine} ${f.name} has no example`);
    }
  }
});

test("available drops what the server lacks", () => {
  const items = [{ name: "a" }, { name: "b", requires: "jsonTable" as const }];
  assert.deepEqual(available(items, []).map((i) => i.name), ["a"]);
  assert.deepEqual(available(items, ["jsonTable"]).map((i) => i.name), ["a", "b"]);
  const mysql = dialectOf("mysql").functions;
  assert.ok(!available(mysql, []).some((f) => f.name === "JSON_TABLE"));
  assert.ok(available(mysql, ["jsonTable"]).some((f) => f.name === "JSON_TABLE"));
});

test("function keys are camelCase", () => {
  assert.equal(functionKey("date_trunc"), "dateTrunc");
  assert.equal(functionKey("CHAR_LENGTH"), "charLength");
  assert.equal(functionKey("LOG10"), "log10");
  assert.equal(functionKey("now"), "now");
});

test("every catalog function is described in english and portuguese", () => {
  for (const engine of ENGINES) {
    const english = (en.sqlFunctions as Record<string, Record<string, string>>)[engine];
    const portuguese = (ptBR.sqlFunctions as Record<string, Record<string, string>>)[engine];
    const keys = dialectOf(engine).functions.map((f) => functionKey(f.name));
    for (const key of keys) {
      assert.ok(english[key], `en sqlFunctions.${engine}.${key}`);
      assert.ok(portuguese[key], `pt-BR sqlFunctions.${engine}.${key}`);
    }
    assert.deepEqual(Object.keys(english).sort(), [...keys].sort(), `${engine}: a description without a function`);
  }
});
