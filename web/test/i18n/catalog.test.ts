import { test } from "node:test";
import assert from "node:assert/strict";
import { en } from "../../src/i18n/locales/en.ts";
import { ptBR } from "../../src/i18n/locales/pt-BR.ts";

type Node = string | { [key: string]: Node };

/** Every message of a catalog by dotted key; a plural message is a leaf. */
function leaves(node: Node, prefix = ""): Map<string, string | Record<string, string>> {
  const out = new Map<string, string | Record<string, string>>();
  for (const [key, value] of Object.entries(node as Record<string, Node>)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") out.set(path, value);
    else if (typeof (value as Record<string, unknown>).other === "string") out.set(path, value as Record<string, string>);
    else for (const [k, v] of leaves(value, path)) out.set(k, v);
  }
  return out;
}

const texts = (message: string | Record<string, string>) => (typeof message === "string" ? [message] : Object.values(message));
const names = (message: string | Record<string, string>, pattern: RegExp) => {
  const found = new Set<string>();
  for (const text of texts(message)) for (const match of text.matchAll(pattern)) found.add(match[1]);
  return found;
};
const PARAM = /\{(\w+)\}/g;
const TAG = /<\/?([a-zA-Z][\w-]*)>/g;

const source = leaves(en as unknown as Node);
const portuguese = leaves(ptBR as unknown as Node);

test("portuguese has exactly the keys of english", () => {
  assert.deepEqual([...portuguese.keys()].sort(), [...source.keys()].sort());
});

test("portuguese uses the same parameters as english in every message", () => {
  for (const [key, message] of source) {
    // A plural gets `count` by itself, so a form may leave it out ("Uma linha")
    const wanted = names(message, PARAM);
    const got = names(portuguese.get(key)!, PARAM);
    if (typeof message !== "string") {
      wanted.delete("count");
      got.delete("count");
    }
    assert.deepEqual([...got].sort(), [...wanted].sort(), key);
  }
});

test("portuguese uses the same tags as english", () => {
  for (const [key, message] of source) {
    assert.deepEqual([...names(portuguese.get(key)!, TAG)].sort(), [...names(message, TAG)].sort(), key);
  }
});

test("plural messages have other, and portuguese also has one", () => {
  for (const [key, message] of source) {
    if (typeof message === "string") continue;
    assert.equal(typeof message.other, "string", key);
    const pt = portuguese.get(key) as Record<string, string>;
    assert.equal(typeof pt.one, "string", key);
    assert.equal(typeof pt.other, "string", key);
  }
});

test("no message is empty or padded with spaces", () => {
  for (const catalog of [source, portuguese]) {
    for (const [key, message] of catalog) {
      for (const text of texts(message)) {
        assert.ok(text.length > 0, `${key} is empty`);
        assert.equal(text, text.trim(), `${key} has spaces at an end`);
      }
    }
  }
});

test("keys are camelCase and at most three levels deep", () => {
  for (const key of source.keys()) {
    const parts = key.split(".");
    assert.ok(parts.length <= 3, `${key} is too deep`);
    for (const part of parts) assert.match(part, /^[a-z][a-zA-Z0-9]*$/, key);
  }
});
