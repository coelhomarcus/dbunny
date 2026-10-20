import { test } from "node:test";
import assert from "node:assert/strict";
import { looseText } from "./scan.ts";

const lines = (source: string) => looseText(source).map((found) => found.text);

test("text between tags is loose", () => {
  assert.deepEqual(lines("const a = <p>Hello there</p>;"), ["Hello there"]);
});

test("attributes that show text are loose, the rest are not", () => {
  assert.deepEqual(lines('const a = <input placeholder="Search" />;'), ["Search"]);
  assert.deepEqual(lines('const a = <Row title="Theme" hint="Pick one" />;'), ["Theme", "Pick one"]);
  assert.deepEqual(lines('const a = <input className="p-2 flex" type="text" />;'), []);
});

test("text that goes through t() is not loose", () => {
  assert.deepEqual(lines('const a = <p>{t("a.b")}</p>;'), []);
  assert.deepEqual(lines('const a = <Row title={t("a.b")} />;'), []);
  assert.deepEqual(lines('const a = <Row title={t("a.b", { n: 1 })} />;'), []);
});

test("values and symbols are not text", () => {
  assert.deepEqual(lines("const a = <span>{count}</span>;"), []);
  assert.deepEqual(lines("const a = <kbd>⌘</kbd>;"), []);
  assert.deepEqual(lines("const a = <i>{1 + 2}</i>;"), []);
});

test("strings inside an expression that is a child are loose", () => {
  assert.deepEqual(lines('const a = <p>{ok ? "Yes" : "No"}</p>;'), ["Yes", "No"]);
  assert.deepEqual(lines("const a = <b>{`${n} rows`}</b>;"), [" rows"]);
});

test("elements inside an expression are scanned too", () => {
  assert.deepEqual(lines("const a = <div>{show && <b>Hello</b>}</div>;"), ["Hello"]);
});

test("it reports the line", () => {
  const found = looseText("const a = (\n  <div>\n    <p>Hi</p>\n  </div>\n);");
  assert.equal(found[0].line, 3);
});
