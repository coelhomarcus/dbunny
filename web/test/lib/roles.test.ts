import assert from "node:assert/strict";
import { test } from "node:test";

import { accountOf, hasHosts } from "../../src/lib/roles.ts";

test("a role without a host is written by its name", () => {
  assert.equal(accountOf({ name: "readonly" }), "readonly");
});

test("an account with a host is written user@host", () => {
  assert.equal(accountOf({ name: "ana", host: "%" }), "ana@%");
  assert.equal(accountOf({ name: "ana", host: "10.0.0.%" }), "ana@10.0.0.%");
});

test("the same user on two hosts is two accounts", () => {
  assert.notEqual(accountOf({ name: "ana", host: "%" }), accountOf({ name: "ana", host: "localhost" }));
});

test("hosts are shown only when some account has one", () => {
  assert.equal(hasHosts([{ name: "a" }, { name: "b" }]), false);
  assert.equal(hasHosts([{ name: "a" }, { name: "b", host: "%" }]), true);
  assert.equal(hasHosts([]), false);
});
