import { test } from "node:test";
import assert from "node:assert/strict";
import { parseMarkdown } from "../../src/lib/markdown.ts";

test("a code fence that is still open reads as code up to the end", () => {
  const blocks = parseMarkdown("Here:\n```sql\nSELECT 1");
  assert.deepEqual(blocks, [
    { kind: "paragraph", inlines: [{ kind: "text", text: "Here:" }] },
    { kind: "code", language: "sql", text: "SELECT 1" },
  ]);
});

test("a numbered list is an ordered list", () => {
  const blocks = parseMarkdown("1. first\n2. second\n3. third");
  assert.equal(blocks.length, 1);
  const list = blocks[0];
  assert.equal(list.kind, "list");
  if (list.kind === "list") {
    assert.equal(list.ordered, true);
    assert.equal(list.items.length, 3);
  }
});

test("a bulleted list is not ordered", () => {
  const list = parseMarkdown("- a\n- b")[0];
  assert.equal(list.kind, "list");
  if (list.kind === "list") assert.equal(list.ordered, false);
});

test("a number in the middle of a sentence does not start a list", () => {
  const blocks = parseMarkdown("There are\n2. things to know");
  assert.equal(blocks.length, 1);
  assert.equal(blocks[0].kind, "paragraph");
});

test("inline code, bold and links keep their kinds", () => {
  const [paragraph] = parseMarkdown("**a** and `b` and [c](https://example.com)");
  assert.equal(paragraph.kind, "paragraph");
  if (paragraph.kind === "paragraph") {
    assert.deepEqual(
      paragraph.inlines.map((inline) => inline.kind),
      ["strong", "text", "code", "text", "link"],
    );
  }
});

test("an empty text has no blocks", () => {
  assert.deepEqual(parseMarkdown(""), []);
  assert.deepEqual(parseMarkdown("\n\n"), []);
});

test("only web and mail links are safe to open", async () => {
  const { isWebLink } = await import("../../src/lib/links.ts");
  for (const url of ["https://example.com/a?b=1", "http://localhost:3000", "mailto:a@b.co"]) assert.equal(isWebLink(url), true, url);
  for (const url of ["javascript:alert(1)", "file:///etc/passwd", "smb://host/share", "x-apple.systempreferences:", "ssh://h", "//example.com", "example.com", ""]) {
    assert.equal(isWebLink(url), false, url);
  }
});
