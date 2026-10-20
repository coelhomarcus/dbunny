import { test } from "node:test";
import assert from "node:assert/strict";
import { makeFormatters } from "../../src/i18n/format.ts";

const en = makeFormatters("en");
const pt = makeFormatters("pt-BR");

test("numbers use the separators of the locale", () => {
  assert.equal(en.number(1234.5), "1,234.5");
  assert.equal(pt.number(1234.5), "1.234,5");
  assert.equal(pt.number(0.256, { style: "percent" }), "26%");
});

test("time has no seconds and follows the locale", () => {
  const at = Date.UTC(2026, 9, 20, 14, 5, 33);
  assert.match(en.time(at, { timeZone: "UTC" }), /^0?2:05\s?PM$/);
  assert.equal(pt.time(at, { timeZone: "UTC" }), "14:05");
});

test("a date is written the way the locale writes it", () => {
  const at = Date.UTC(2026, 9, 20, 12);
  assert.match(en.date(at, { timeZone: "UTC" }), /Oct.*20.*2026/);
  assert.match(pt.date(at, { timeZone: "UTC" }), /20.*out.*2026/);
});

test("relative says minutes ago in both languages", () => {
  const now = Date.UTC(2026, 9, 20, 12, 0, 0);
  const then = now - 5 * 60_000;
  assert.equal(en.relative(then, now), "5 minutes ago");
  assert.equal(pt.relative(then, now), "há 5 minutos");
});

test("relative picks the biggest unit that fits", () => {
  const now = Date.UTC(2026, 9, 20, 12, 0, 0);
  assert.equal(en.relative(now - 3 * 3_600_000, now), "3 hours ago");
  assert.equal(en.relative(now - 2 * 86_400_000, now), "2 days ago");
  assert.equal(en.relative(now, now), "now");
});

test("list joins with and / e", () => {
  assert.equal(en.list(["A", "B", "C"]), "A, B, and C");
  assert.equal(pt.list(["A", "B", "C"]), "A, B e C");
  assert.equal(en.list(["A", "B"]), "A and B");
  assert.equal(pt.list(["A", "B"]), "A e B");
});

test("bytes keeps the rule of lib/format with the decimal separator of the locale", () => {
  assert.equal(en.bytes(2), "2 B");
  assert.equal(en.bytes(1024 * 1.4), "1.4 kB");
  assert.equal(pt.bytes(1024 * 1.4), "1,4 kB");
  assert.equal(en.bytes(1024 * 250), "250 kB");
  assert.equal(en.bytes(5 * 1024 * 1024), "5.0 MB");
});

test("a formatter is built once for a language and its options, not on every call", () => {
  const original = Intl.NumberFormat;
  let built = 0;
  const counting = function (this: unknown, ...args: ConstructorParameters<typeof Intl.NumberFormat>) {
    built++;
    return new original(...args);
  } as unknown as typeof Intl.NumberFormat;
  Intl.NumberFormat = counting;
  try {
    const fresh = makeFormatters("pt-BR");
    for (let i = 0; i < 5; i++) fresh.number(1234.5, { maximumFractionDigits: 7 });
    assert.equal(built, 1);
    fresh.bytes(2048);
    const afterFirstSize = built;
    for (let i = 0; i < 5; i++) fresh.bytes(2048);
    assert.equal(built, afterFirstSize, "sizes reuse theirs too");
    fresh.number(1, { maximumFractionDigits: 8 });
    assert.equal(built, afterFirstSize + 1, "other options get their own");
  } finally {
    Intl.NumberFormat = original;
  }
});

test("the same language gives the same results after the formatters are reused", () => {
  const at = Date.UTC(2026, 9, 20, 14, 5);
  assert.equal(en.time(at, { timeZone: "UTC" }), en.time(at, { timeZone: "UTC" }));
  assert.equal(pt.list(["A", "B"]), pt.list(["A", "B"]));
  assert.notEqual(en.date(at, { timeZone: "UTC" }), pt.date(at, { timeZone: "UTC" }));
});
