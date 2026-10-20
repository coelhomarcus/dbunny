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

test("a string that is compared is not text", () => {
  assert.deepEqual(lines('const a = <div>{items.map((i) => (i === "separator" ? <hr key={i} /> : <b key={i} />))}</div>;'), []);
  assert.deepEqual(lines('const a = <p>{kind !== "table" && <hr />}</p>;'), []);
});

test("keywords, brand names and code are not text to translate", () => {
  assert.deepEqual(lines("const a = <span>NULL</span>;"), []);
  assert.deepEqual(lines("const a = <b>PARTITION BY</b>;"), []);
  assert.deepEqual(lines("const a = <b>DBunny</b>;"), []);
  assert.deepEqual(lines("const a = <b>PostgreSQL, MySQL</b>;"), []);
  assert.deepEqual(lines("const a = <code>created_at</code>;"), []);
  assert.deepEqual(lines("const a = <pre>some text</pre>;"), []);
  assert.deepEqual(lines("const a = <b>Some text</b>;"), ["Some text"]);
});

test("strings in a callback body that don't reach the screen are not text", () => {
  assert.deepEqual(lines('const a = <div>{blocks.map((block) => { const Tag = block.level <= 1 ? "h3" : "h4"; return <Tag>{block.text}</Tag>; })}</div>;'), []);
  assert.deepEqual(lines('const a = <div>{expanded.has("pinned-closed") && <hr />}</div>;'), []);
  assert.deepEqual(lines('const a = <div>{"error" in current && <hr />}</div>;'), []);
  assert.deepEqual(lines("const a = <div>{rows.map((r) => <i key={`s:${r.id}`}>{r.name}</i>)}</div>;"), []);
});

test("what a callback or a list hands to the screen is text", () => {
  assert.deepEqual(lines('const a = <div>{list.map((x) => (x.ok ? "Yes" : "No"))}</div>;'), ["Yes", "No"]);
  assert.deepEqual(lines('const a = <tr>{["Name", "Type"].map((h) => <th key={h}>{h}</th>)}</tr>;'), ["Name", "Type"]);
  assert.deepEqual(lines('const a = <div>{rows.map((r) => { return r.ok ? "Fine" : <b>Bad</b>; })}</div>;'), ["Fine", "Bad"]);
  assert.deepEqual(lines('const a = <b>{name || "Connection"}</b>;'), ["Connection"]);
});

test("a text attribute with an expression is checked too", () => {
  assert.deepEqual(lines('const a = <b aria-label={ok ? "Yes" : "No"} />;'), ["Yes", "No"]);
  assert.deepEqual(lines("const a = <b title={`Close ${name}`} />;"), ["Close "]);
  assert.deepEqual(lines('const a = <b aria-label={t("a.b")} />;'), []);
});

test("a prop with a capitalized sentence or word is text, whatever its name", () => {
  assert.deepEqual(lines('const a = <ConfirmDialog confirmLabel="Discard and close" />;'), ["Discard and close"]);
  assert.deepEqual(lines('const a = <Thing emptyText="Nothing yet" />;'), ["Nothing yet"]);
  assert.deepEqual(lines('const a = <Thing variant="primary" side="top" className="Foo bar" type="button" />;'), []);
  assert.deepEqual(lines('const a = <Thing data-state="Open" onClick={x} />;'), []);
});
