import { test } from "node:test";
import assert from "node:assert/strict";
import type { ColumnInfo } from "@/types";
import { editorFor } from "../../src/lib/cellEdit.ts";

const column = (dataType: string): ColumnInfo => ({ name: "c", dataType, typeName: dataType, isNullable: true, defaultValue: null, ordinalPosition: 1, isPrimaryKey: false, isGenerated: false, comment: null, enumValues: null, foreignKey: null });

test("date columns open the calendar for date text and for nothing", () => {
  assert.deepEqual(editorFor(column("date"), "2026-10-16"), { kind: "datetime", mode: "date" });
  assert.deepEqual(editorFor(column("date"), null), { kind: "datetime", mode: "date" });
});

test("numbers in date columns are edited as text", () => {
  // SQLite keeps dates as Unix times in an integer: the calendar would turn them into date text
  assert.deepEqual(editorFor(column("date"), 1760000000), { kind: "text" });
  assert.deepEqual(editorFor(column("timestamp without time zone"), 1760000000.5), { kind: "text" });
});

test("other column kinds are unchanged", () => {
  assert.deepEqual(editorFor(column("boolean"), true), { kind: "choice", options: ["true", "false"] });
  assert.deepEqual(editorFor(column("text"), "short"), { kind: "text" });
  assert.deepEqual(editorFor(column("jsonb"), "{}"), { kind: "long", json: true });
});
