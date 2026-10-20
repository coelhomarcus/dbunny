import { test } from "node:test";
import assert from "node:assert/strict";
import { interpolate, lookup, parseRich, pick, translate } from "../../src/i18n/translate.ts";

test("interpolate replaces named parameters", () => {
  assert.equal(interpolate("Hello {name}, {n} left", { name: "Ana", n: 3 }), "Hello Ana, 3 left");
});

test("interpolate leaves a missing parameter visible", () => {
  assert.equal(interpolate("Hi {name}"), "Hi {name}");
  assert.equal(interpolate("Hi {name}", { other: 1 }), "Hi {name}");
});

test("pick uses one and other for english", () => {
  const rows = { one: "{count} row", other: "{count} rows" };
  assert.equal(pick(rows, 1, "en"), "{count} row");
  assert.equal(pick(rows, 0, "en"), "{count} rows");
  assert.equal(pick(rows, 2, "en"), "{count} rows");
});

test("pick follows the portuguese rules", () => {
  const rows = { one: "{count} linha", other: "{count} linhas" };
  assert.equal(pick(rows, 1, "pt-BR"), "{count} linha");
  assert.equal(pick(rows, 0, "pt-BR"), "{count} linha");
  assert.equal(pick(rows, 2, "pt-BR"), "{count} linhas");
});

test("pick falls back to other for a category the message does not have", () => {
  assert.equal(pick({ other: "x" }, 1000000, "pt-BR"), "x");
  assert.equal(pick({ other: "x" }, 1, "en"), "x");
});

test("pick passes a plain string through", () => {
  assert.equal(pick("Save", undefined, "en"), "Save");
  assert.equal(pick("Save", 3, "en"), "Save");
});

test("lookup finds nested keys in the first catalog that has them", () => {
  const a = { common: { save: "Salvar" } };
  const b = { common: { save: "Save", cancel: "Cancel" } };
  assert.equal(lookup([a, b], "common.save"), "Salvar");
  assert.equal(lookup([a, b], "common.cancel"), "Cancel");
  assert.equal(lookup([a, b], "common.nope"), undefined);
  assert.equal(lookup([a, b], "common"), undefined);
});

test("translate walks the fallback chain", () => {
  const pt = { other: { x: "ex" } };
  const en = { common: { save: "Save" } };
  assert.equal(translate([pt, en], "en", "common.save"), "Save");
  assert.equal(translate([pt, en], "en", "common.missing"), "common.missing");
});

test("translate fills count into the plural message", () => {
  const cat = { rows: { one: "{count} linha", other: "{count} linhas" } };
  assert.equal(translate([cat], "pt-BR", "rows", { count: 3 }), "3 linhas");
  assert.equal(translate([cat], "pt-BR", "rows", { count: 1 }), "1 linha");
});

test("parseRich splits tags and text", () => {
  assert.deepEqual(parseRich("a <b>b <i>c</i></b> d"), [
    "a ",
    { tag: "b", children: ["b ", { tag: "i", children: ["c"] }] },
    " d",
  ]);
});

test("parseRich keeps text that only looks like markup", () => {
  assert.deepEqual(parseRich("1 < 2"), ["1 < 2"]);
  assert.deepEqual(parseRich("<b>open"), ["<b>open"]);
  assert.deepEqual(parseRich("close</b>"), ["close</b>"]);
  assert.deepEqual(parseRich("Hi {name}"), ["Hi {name}"]);
});

test("count is written with the separators of the language", () => {
  const cat = { rows: { one: "{count} row", other: "{count} rows" } };
  assert.equal(translate([cat], "en", "rows", { count: 1234 }), "1,234 rows");
  const pt = { rows: { one: "{count} linha", other: "{count} linhas" } };
  assert.equal(translate([pt], "pt-BR", "rows", { count: 1234 }), "1.234 linhas");
});

test("other numbers are written as they are", () => {
  assert.equal(translate([{ year: "Year {year}" }], "pt-BR", "year", { year: 2026 }), "Year 2026");
});
