import { test } from "node:test";
import assert from "node:assert/strict";
import { buildDiagramGraph } from "../../src/lib/diagramGraph.ts";
import type { Diagram } from "../../src/types/schema.ts";

const diagram: Diagram = {
  tables: [
    {
      schema: "public",
      name: "posts",
      columns: [
        { name: "id", typeName: "integer", isPrimaryKey: true, isForeignKey: false, ordinal: 1 },
        { name: "country", typeName: "text", isPrimaryKey: false, isForeignKey: true, ordinal: 2 },
        { name: "region", typeName: "text", isPrimaryKey: false, isForeignKey: true, ordinal: 3 },
      ],
    },
    {
      schema: "public",
      name: "regions",
      columns: [
        { name: "country", typeName: "text", isPrimaryKey: true, isForeignKey: true, ordinal: 1 },
        { name: "code", typeName: "text", isPrimaryKey: true, isForeignKey: true, ordinal: 2 },
      ],
    },
    {
      schema: "billing",
      name: "accounts",
      columns: [{ name: "id", typeName: "integer", isPrimaryKey: true, isForeignKey: true, ordinal: 1 }],
    },
  ],
  relations: [
    {
      name: "posts_region_fk",
      fromSchema: "public",
      fromTable: "posts",
      fromColumns: ["country", "region"],
      toSchema: "public",
      toTable: "regions",
      toColumns: ["country", "code"],
    },
    {
      name: "posts_account_fk",
      fromSchema: "public",
      fromTable: "posts",
      fromColumns: ["id"],
      toSchema: "billing",
      toTable: "accounts",
      toColumns: ["id"],
    },
    {
      name: "dangling",
      fromSchema: "public",
      fromTable: "posts",
      fromColumns: ["id"],
      toSchema: "gone",
      toTable: "missing",
      toColumns: ["id"],
    },
  ],
};

test("a composite key uses the first column and labels the rest", () => {
  const { edges } = buildDiagramGraph(diagram);
  const region = edges.find((edge) => edge.label !== "");
  assert.ok(region);
  assert.equal(region.sourceHandle, "country");
  assert.equal(region.targetHandle, "country");
  assert.equal(region.label, "region → code");
  assert.equal(region.source.endsWith("\u0000posts"), true);
  assert.equal(region.target.endsWith("\u0000regions"), true);
});

test("a key into another schema is one edge, and a missing table is dropped", () => {
  const { nodes, edges } = buildDiagramGraph(diagram);
  assert.equal(nodes.length, 3);
  const cross = edges.find((edge) => edge.target.startsWith("billing"));
  assert.ok(cross);
  assert.equal(cross.label, "");
  assert.equal(cross.sourceHandle, "id");
  assert.equal(cross.targetHandle, "id");
  assert.equal(edges.length, 2);
});
