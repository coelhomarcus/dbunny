import { test } from "node:test";
import assert from "node:assert/strict";
import type { MysqlPlanNode } from "@/types";
import { mysqlPlanRows, sqlitePlanRows } from "../../src/lib/plan.ts";

test("sqlite plan rows nest by parent, in reading order", () => {
  const rows = sqlitePlanRows([
    { id: 2, parent: 0, detail: "SCAN orders" },
    { id: 3, parent: 0, detail: "SEARCH users USING INTEGER PRIMARY KEY (rowid=?)" },
    { id: 5, parent: 3, detail: "USE TEMP B-TREE FOR ORDER BY" },
    { id: 9, parent: 5, detail: "SCAN x" },
    { id: 10, parent: 0, detail: "LIST SUBQUERY 1" },
  ]);
  assert.deepEqual(
    rows.map((r) => [r.depth, r.detail]),
    [
      [0, "SCAN orders"],
      [0, "SEARCH users USING INTEGER PRIMARY KEY (rowid=?)"],
      [1, "USE TEMP B-TREE FOR ORDER BY"],
      [2, "SCAN x"],
      [0, "LIST SUBQUERY 1"],
    ],
  );
});

test("a sqlite plan row whose parent is missing is shown at the top", () => {
  const rows = sqlitePlanRows([{ id: 4, parent: 99, detail: "SCAN t" }]);
  assert.deepEqual(rows.map((r) => r.depth), [0]);
});

test("an empty sqlite plan has no rows", () => {
  assert.deepEqual(sqlitePlanRows([]), []);
});

const mysqlNode = (label: string, estimate: { cost: number; rows: number } | null, actual: { first: number; last: number; rows: number; loops: number } | null, children: MysqlPlanNode[] = []): MysqlPlanNode => ({
  label,
  cost: estimate?.cost ?? null,
  rows: estimate?.rows ?? null,
  actualTimeFirst: actual?.first ?? null,
  actualTimeLast: actual?.last ?? null,
  actualRows: actual?.rows ?? null,
  loops: actual?.loops ?? null,
  children,
});

// EXPLAIN ANALYZE of a join: sort <- stream <- hash join <- (filter <- scan a, hash <- scan b)
const analyzed: MysqlPlanNode[] = [
  mysqlNode("Sort: b.`name`", null, { first: 0.37, last: 0.37, rows: 3, loops: 1 }, [
    mysqlNode("Stream results", { cost: 1, rows: 0.667 }, { first: 0.349, last: 0.351, rows: 3, loops: 1 }, [
      mysqlNode("Inner hash join (a.b_id = b.id)", { cost: 1, rows: 0.667 }, { first: 0.335, last: 0.337, rows: 3, loops: 1 }, [
        mysqlNode("Filter: (a.v <> 'z')", { cost: 0.225, rows: 1 }, { first: 0.00387, last: 0.00487, rows: 3, loops: 1 }, [
          mysqlNode("Table scan on a", { cost: 0.225, rows: 3 }, { first: 0.00258, last: 0.00325, rows: 3, loops: 1 }),
        ]),
        mysqlNode("Hash", null, null, [mysqlNode("Table scan on b", { cost: 0.45, rows: 2 }, { first: 0.306, last: 0.307, rows: 2, loops: 1 })]),
      ]),
    ]),
  ]),
];

test("a mysql tree becomes plan rows in reading order, each with its depth", () => {
  const rows = mysqlPlanRows(analyzed);
  assert.deepEqual(
    rows.map((r) => [r.depth, r.title]),
    [
      [0, "Sort: b.`name`"],
      [1, "Stream results"],
      [2, "Inner hash join (a.b_id = b.id)"],
      [3, "Filter: (a.v <> 'z')"],
      [4, "Table scan on a"],
      [3, "Hash"],
      [4, "Table scan on b"],
    ],
  );
});

test("an analyzed mysql node shows its time and rows", () => {
  const join = mysqlPlanRows(analyzed)[2];
  assert.equal(join.totalMs, 0.337);
  assert.equal(join.actualRows, 3);
  assert.equal(join.cost, 1);
  assert.equal(join.estimatedRows, 0.667);
  assert.equal(join.misestimate, null);
  // Its own time leaves out what its children took (the hash node never reported one)
  assert.ok(Math.abs((join.selfMs ?? 0) - (0.337 - 0.00487)) < 1e-9, String(join.selfMs));
  // A node that reported nothing has no time
  const hash = mysqlPlanRows(analyzed)[5];
  assert.deepEqual([hash.totalMs, hash.selfMs, hash.actualRows, hash.cost], [null, null, null, null]);
});

test("a mysql node that loops counts every loop", () => {
  const [row] = mysqlPlanRows([mysqlNode("Index lookup on t", { cost: 1, rows: 2 }, { first: 0.1, last: 2, rows: 4, loops: 3 })]);
  assert.equal(row.totalMs, 6);
  assert.equal(row.actualRows, 12);
  assert.equal(row.estimatedRows, 6);
});

test("a mysql row estimate that is far off is flagged", () => {
  const [row] = mysqlPlanRows([mysqlNode("Table scan on t", { cost: 1, rows: 2 }, { first: 0, last: 1, rows: 500, loops: 1 })]);
  assert.equal(row.misestimate, 250);
});

test("a plan that was not run has estimates only", () => {
  const rows = mysqlPlanRows([mysqlNode("Table scan on t", { cost: 3.5, rows: 10 }, null)]);
  assert.deepEqual([rows[0].cost, rows[0].estimatedRows, rows[0].actualRows, rows[0].totalMs, rows[0].misestimate], [3.5, 10, null, null, null]);
});

test("an empty mysql plan has no rows", () => {
  assert.deepEqual(mysqlPlanRows([]), []);
});
