import { test } from "node:test";
import assert from "node:assert/strict";
import { sqlitePlanRows } from "../../src/lib/plan.ts";

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
