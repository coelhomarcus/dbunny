import { test } from "node:test";
import assert from "node:assert/strict";
import { readSeen, rememberSeen, seenValue, shouldShowSupportNotice, supportNoticeKey } from "../../src/lib/supportNotice.ts";

const eos = { line: "13", date: "2025-11-13" };

test("the notice shows once per line", () => {
  assert.equal(shouldShowSupportNotice(eos, null, "postgres"), true);
  assert.equal(shouldShowSupportNotice(eos, seenValue("postgres", eos), "postgres"), false);
  assert.equal(shouldShowSupportNotice(null, null, "postgres"), false);
});

test("a new line shows the notice again", () => {
  assert.equal(shouldShowSupportNotice({ line: "14", date: "2026-11-12" }, seenValue("postgres", eos), "postgres"), true);
});

test("keys follow the saved connection or the address", () => {
  assert.equal(supportNoticeKey("abc", { engine: "postgres", host: "h", port: 5432 }), "dbunny.endOfSupportSeen.abc");
  assert.equal(supportNoticeKey(null, { engine: "mysql", host: "h", port: 3306 }), "dbunny.endOfSupportSeen.mysql:h:3306");
});

test("seen values round trip through storage", () => {
  const store = new Map<string, string>();
  const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
  rememberSeen(storage, "k", seenValue("mariadb", { line: "10.6", date: "2026-07-06" }));
  assert.equal(readSeen(storage, "k"), "mariadb 10.6");
});

test("storage failures show the notice", () => {
  const broken = {
    getItem: (): string | null => {
      throw new Error("denied");
    },
    setItem: (): void => {
      throw new Error("denied");
    },
  };
  assert.equal(readSeen(broken, "k"), null);
  assert.doesNotThrow(() => rememberSeen(broken, "k", "v"));
  assert.equal(readSeen(null, "k"), null);
});
