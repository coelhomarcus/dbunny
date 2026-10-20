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

test("object properties that carry words are text, in a tsx or a ts file", () => {
  assert.deepEqual(lines('const tabs = [{ value: "a", label: "Connection" }, { value: "b", label: "SSH agent" }];'), ["Connection", "SSH agent"]);
  assert.deepEqual(lines('const a = <Tabs options={[{ value: "require", label: "Required" }]} />;'), ["Required"]);
  assert.deepEqual(looseText('notify({ text: "Saved the query", tone: "success" });', "x.ts").map((found) => found.text), ["Saved the query"]);
  assert.deepEqual(looseText('const a = { label: t("a.b"), name: "Postgres", value: "x" };', "x.ts"), []);
  assert.deepEqual(looseText('const a = { title: ok ? "Yes" : "No" };', "x.ts").map((found) => found.text), ["Yes", "No"]);
  assert.deepEqual(looseText('const a = { label: "NULL", description: "SQL" };', "x.ts"), []);
});

test("a ts file is read for words in those properties and for sentences, nothing else", () => {
  assert.deepEqual(looseText('export const x = "some words"; function f() { return "more words"; }', "x.ts"), []);
});

test("identifiers are not text", () => {
  assert.deepEqual(lines('const a = <b>{["sslRootCert", t("a.b")].map((k) => k)}</b>;'), []);
  assert.deepEqual(lines('const a = <b>{cond ? "snake_case" : "kebab-case"}</b>;'), []);
  assert.deepEqual(lines('const a = <b>{cond ? "empty" : "Full"}</b>;'), ["empty", "Full"]);
});

test("colors are not text", () => {
  assert.deepEqual(looseText('const theme = { text: "#f4f4f5", label: "#18181b" };', "x.ts"), []);
});

test("units are not text", () => {
  assert.deepEqual(lines("const a = <b>{n} ms</b>;"), []);
  assert.deepEqual(lines("const a = <b>{n} kB</b>;"), []);
});

test("a sentence kept anywhere in the code is text", () => {
  assert.deepEqual(looseText('let title = "Sign in with your agent, then try again.";', "x.ts").map((found) => found.text), ["Sign in with your agent, then try again."]);
  assert.deepEqual(looseText('const m = { a: ok ? "The assistant stopped" : "Nothing was logged." };', "x.ts").map((found) => found.text), ["The assistant stopped", "Nothing was logged."]);
  assert.deepEqual(looseText('const s = `Sign out of ${name}?`;', "x.ts").map((found) => found.text), ["Sign out of "]);
});

test("logic and developer text are not user text", () => {
  assert.deepEqual(looseText('if (kind === "Some Thing") {}\nconsole.warn("Something went wrong here");\nthrow new Error("It broke badly");', "x.ts"), []);
  assert.deepEqual(looseText('import x from "Some Package";\nconst a = <b className="Foo bar baz" d="M 0 0" />;', "x.tsx"), []);
  assert.deepEqual(looseText('const a = t("Some key here");', "x.ts"), []);
  assert.deepEqual(looseText('const a = "Cancel";', "x.ts"), []);
});

test("names of languages are not translated", () => {
  assert.deepEqual(looseText('const a = "Português (Brasil)";', "x.ts"), []);
});

test("a line marked i18n-ignore is left alone", () => {
  assert.deepEqual(looseText('export const SENT = "Ask them how to continue."; // i18n-ignore: said to the agent', "x.ts"), []);
  assert.deepEqual(looseText('export const SENT = "Ask them how to continue.";', "x.ts").map((found) => found.text), ["Ask them how to continue."]);
});

test("a string used as a key to read a value is not text", () => {
  assert.deepEqual(looseText('const a = top["Execution Time"] ?? top["Planning Time"];', "x.ts"), []);
  assert.deepEqual(lines('const a = <Keys keys="Esc" />;'), []);
});

test("a sentence whose first word has an apostrophe is text", () => {
  assert.deepEqual(looseText('const m = "Couldn\'t read the roles";', "x.ts").map((found) => found.text), ["Couldn't read the roles"]);
});
