import { test } from "node:test";
import assert from "node:assert/strict";
import { pseudoCatalog, pseudolocalize } from "../../src/i18n/pseudo.ts";

test("pseudolocalize keeps parameters and tags", () => {
  const out = pseudolocalize("Hi {name} <b>there</b>");
  assert.ok(out.includes("{name}"));
  assert.ok(out.includes("<b>"));
  assert.ok(out.includes("</b>"));
});

test("it is longer than the source and marked", () => {
  const out = pseudolocalize("Cancel");
  assert.ok(out.length > "Cancel".length * 1.3);
  assert.ok(out.startsWith("[") && out.endsWith("]"));
  assert.notEqual(out, "Cancel");
});

test("a catalog keeps its shape, plural forms included", () => {
  const out = pseudoCatalog({ a: { b: "Save", rows: { one: "{count} row", other: "{count} rows" } } }) as {
    a: { b: string; rows: { one: string; other: string } };
  };
  assert.ok(out.a.b.startsWith("["));
  assert.ok(out.a.rows.one.includes("{count}"));
  assert.ok(out.a.rows.other.includes("{count}"));
});
