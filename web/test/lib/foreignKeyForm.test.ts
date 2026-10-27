import { test } from "node:test";
import assert from "node:assert/strict";
import type { CompletionRelation } from "@/types";
import { foreignKeyDraftError, foreignKeysForSpec } from "../../src/lib/foreignKeyForm.ts";

const users: CompletionRelation = {
  schema: "public",
  name: "users",
  type: "table",
  columns: [
    { name: "id", dataType: "bigint", primaryKey: true, nullable: false, references: null },
    { name: "email", dataType: "text", primaryKey: false, nullable: true, references: null },
  ],
};

test("foreignKeyDraftError asks for a table and for columns", () => {
  assert.equal(foreignKeyDraftError({ refTable: "", columns: [], refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" }, ["user_id"]), "Choose a table");
  assert.equal(
    foreignKeyDraftError({ refTable: "public.users", columns: [], refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" }, ["user_id"]),
    "Choose at least one column",
  );
  assert.equal(
    foreignKeyDraftError({ refTable: "public.users", columns: ["user_id"], refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" }, ["user_id"]),
    null,
  );
});

test("foreignKeysForSpec skips incomplete drafts and builds complete ones", () => {
  const drafts = [
    { refTable: "", columns: ["a"], refColumns: {}, onUpdate: "NO ACTION", onDelete: "CASCADE" },
    { refTable: "public.users", columns: ["user_id"], refColumns: {}, onUpdate: "NO ACTION", onDelete: "CASCADE" },
  ];
  assert.deepEqual(foreignKeysForSpec(drafts, [users], ["user_id"]), [
    {
      columns: ["user_id"],
      refSchema: "public",
      refTable: "users",
      refColumns: ["id"],
      onUpdate: "NO ACTION",
      onDelete: "CASCADE",
    },
  ]);
});
