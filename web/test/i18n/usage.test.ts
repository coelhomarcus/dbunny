import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { en } from "../../src/i18n/locales/en.ts";

const ROOT = join(import.meta.dirname, "../..");

/** Keys that are used through a typed map or a code from the backend, so no `t("…")` names them. */
const DYNAMIC_KEYS: string[] = [];
/** Messages found by a code from the backend (`errors.<code>`) or by a catalog entry (`sqlFunctions.<engine>.<key>`, `sqlSnippets.<id>`), so no `t("…")` names them. */
const DYNAMIC_PREFIXES = ["errors.", "sqlFunctions.", "sqlSnippets."];
/** Keys written ahead of the screens that will use them; each leaves this list when it is used. */
const PENDING_KEYS: string[] = [];

function sources(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...sources(path));
    else if (/\.tsx?$/.test(entry.name) && !path.includes(`${join("src", "i18n", "locales")}`)) out.push(readFileSync(path, "utf8"));
  }
  return out;
}

function keysOf(node: unknown, prefix = ""): string[] {
  return Object.entries(node as Record<string, unknown>).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string" || typeof (value as { other?: unknown }).other === "string") return [path];
    return keysOf(value, path);
  });
}

// t("a.b") and <Rich id="a.b" />, as written in the code
const CALLED = /(?<![\w.])t\(\s*"([\w.]+)"|<Rich\s[^>]*?\bid="([\w.]+)"/g;
const texts = sources(join(ROOT, "src"));
const called = new Set<string>();
for (const text of texts) {
  for (const match of text.matchAll(CALLED)) called.add(match[1] ?? match[2]);
}
const known = new Set(keysOf(en));
// A key is used when its name is written in the code: in a call, a condition between two keys, or a typed map
const used = new Set([...known].filter((key) => texts.some((text) => text.includes(`"${key}"`))));

test("every static t() key exists in english", () => {
  assert.deepEqual([...called].filter((key) => !known.has(key)).sort(), []);
});

test("every key of english is used, or is waiting for its screen", () => {
  const unused = [...known].filter((key) => !used.has(key) && !DYNAMIC_KEYS.includes(key) && !DYNAMIC_PREFIXES.some((prefix) => key.startsWith(prefix)) && !PENDING_KEYS.includes(key));
  assert.deepEqual(unused.sort(), [], "a key nobody uses is dead text: remove it");
});

test("a key that waits for its screen is not used yet", () => {
  assert.deepEqual(PENDING_KEYS.filter((key) => used.has(key)), [], "these are used now: take them off PENDING_KEYS");
});

test("the keys listed as dynamic or pending exist", () => {
  assert.deepEqual([...DYNAMIC_KEYS, ...PENDING_KEYS].filter((key) => !known.has(key)), []);
});
