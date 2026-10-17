import { test } from "node:test";
import assert from "node:assert/strict";
import type { ColumnInfo } from "@/types";
import { collectInsertValues } from "../../src/lib/insertRowValues.ts";

const column = (name: string): ColumnInfo => ({
  name,
  typeName: "text",
  dataType: "text",
  isNullable: true,
  defaultValue: null,
  isPrimaryKey: false,
  isGenerated: false,
  enumValues: null,
  ordinalPosition: 1,
  comment: null,
  foreignKey: null,
});

test("collectInsertValues omits untouched columns", () => {
  assert.deepEqual(collectInsertValues([column("a"), column("b")], {}), {});
});

test("collectInsertValues sends SQL NULL and touched text", () => {
  assert.deepEqual(
    collectInsertValues([column("a"), column("b")], {
      a: { touched: true, isNull: true, text: "" },
      b: { touched: true, isNull: false, text: "hi" },
    }),
    { a: null, b: "hi" },
  );
});

test("collectInsertValues sends SQL NULL when the null flag is on", () => {
  assert.deepEqual(collectInsertValues([column("a")], { a: { touched: false, isNull: true, text: "x" } }), { a: null });
});
