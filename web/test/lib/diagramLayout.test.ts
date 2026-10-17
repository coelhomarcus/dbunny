import { test } from "node:test";
import assert from "node:assert/strict";
import { arrangeTables, columnCenterY, relationLabelAt, relationRoute } from "../../src/lib/diagramLayout.ts";

test("arrange puts the referenced table to the right of the one that points at it", () => {
  const positions = arrangeTables(
    [
      { id: "posts", columnCount: 3, showSchema: false },
      { id: "users", columnCount: 2, showSchema: true },
    ],
    [{ source: "posts", target: "users" }],
  );
  const posts = positions.get("posts");
  const users = positions.get("users");
  assert.ok(posts && users);
  assert.ok(posts.x < users.x, `${posts.x} should be left of ${users.x}`);
  assert.notEqual(posts.y, undefined);
});

test("a column anchor sits in the middle of its row", () => {
  assert.equal(columnCenterY(0, false), 32 + 11);
  assert.equal(columnCenterY(1, true), 32 + 16 + 22 + 11);
});

test("a key drawn backwards routes around the cards", () => {
  const forward = relationRoute(0, 10, 200, 40);
  assert.equal(forward.length, 4);
  assert.ok(forward.every((point) => point.x >= 0 && point.x <= 200));

  const backward = relationRoute(200, 10, 0, 80);
  assert.ok(backward.some((point) => point.x > 200));
  assert.ok(backward.some((point) => point.x < 0));
  const label = relationLabelAt(backward);
  assert.equal(label.y, (10 + 80) / 2);
});
