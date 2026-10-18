import { test } from "node:test";
import assert from "node:assert/strict";
import { parseReleaseMarkdown } from "../../src/lib/releaseMarkdown.ts";

const notes = `
DBunny 0.7.0 opens a schema as a diagram.

## What's new

- A diagram of one schema, with a key that points at another schema.
- The explorer no longer shows object counts.

## macOS

Clear it once:

\`\`\`bash
xattr -dr com.apple.quarantine "/Applications/DBunny.app"
\`\`\`

## Which file do I need?

| Your system | Download |
|---|---|
| macOS Apple Silicon | \`DBunny-0.7.0-macOS-Apple-Silicon.dmg\` |
| Windows | \`DBunny-0.7.0-Windows-x64-setup.exe\` (or the \`.msi\`) |

See the [release](https://example.com/v0.7.0) for **details**.
`;

test("a release changelog keeps headings, lists, code, tables and inline marks", () => {
  const blocks = parseReleaseMarkdown(notes);
  assert.deepEqual(
    blocks.map((block) => block.kind),
    ["paragraph", "heading", "list", "heading", "paragraph", "code", "heading", "table", "paragraph"],
  );

  const list = blocks[2];
  assert.equal(list.kind, "list");
  if (list.kind === "list") assert.equal(list.items.length, 2);

  const code = blocks[5];
  assert.equal(code.kind, "code");
  if (code.kind === "code") {
    assert.equal(code.language, "bash");
    assert.match(code.text, /xattr -dr/);
  }

  const table = blocks[7];
  assert.equal(table.kind, "table");
  if (table.kind === "table") {
    assert.deepEqual(
      table.header.map((cell) => cell.map((part) => part.text).join("")),
      ["Your system", "Download"],
    );
    assert.equal(table.rows.length, 2);
    assert.deepEqual(table.rows[0][1], [{ kind: "code", text: "DBunny-0.7.0-macOS-Apple-Silicon.dmg" }]);
    assert.deepEqual(table.rows[1][1][0], { kind: "code", text: "DBunny-0.7.0-Windows-x64-setup.exe" });
    assert.deepEqual(table.rows[1][1][2], { kind: "code", text: ".msi" });
  }

  const last = blocks[8];
  assert.equal(last.kind, "paragraph");
  if (last.kind === "paragraph") {
    assert.deepEqual(last.inlines, [
      { kind: "text", text: "See the " },
      { kind: "link", text: "release", href: "https://example.com/v0.7.0" },
      { kind: "text", text: " for " },
      { kind: "strong", text: "details" },
      { kind: "text", text: "." },
    ]);
  }
});

test("an empty changelog has no blocks", () => {
  assert.deepEqual(parseReleaseMarkdown("  \n\n"), []);
});

test("a wrapped list item stays one item", () => {
  const [list] = parseReleaseMarkdown("- first line\n  continued\n- second");
  assert.equal(list.kind, "list");
  if (list.kind === "list") {
    assert.equal(list.items.length, 2);
    assert.equal(list.items[0].map((part) => (part.kind === "text" ? part.text : "")).join(""), "first line continued");
  }
});
