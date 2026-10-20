import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { filesWithLooseText } from "./scan.ts";

const ROOT = join(import.meta.dirname, "../..");
const config: { files: string[]; keep: Record<string, string> } = JSON.parse(readFileSync(join(import.meta.dirname, "literalExceptions.json"), "utf8"));
const loose = filesWithLooseText(ROOT);
const keep = Object.keys(config.keep);

test("no file has loose text unless it is on the exception list", () => {
  const unlisted = [...loose.entries()].filter(([file]) => !config.files.includes(file) && !keep.includes(file));
  const report = unlisted.map(([file, found]) => `${file}:${found[0].line} "${found[0].text}"`);
  assert.deepEqual(report, [], "text in the interface has to go through t(); see web/src/i18n (or mark a line // i18n-ignore: why)");
});

test("the exception list only holds files that still have loose text", () => {
  const stale = config.files.filter((file) => !existsSync(join(ROOT, file)) || !loose.has(file));
  assert.deepEqual(stale, [], "these files are clean now: take them off literalExceptions.json");
});

test("a file kept in English on purpose says why, and still has the text", () => {
  for (const [file, reason] of Object.entries(config.keep)) {
    assert.ok(reason.trim().length > 10, `${file} needs a reason`);
    assert.ok(loose.has(file), `${file} has nothing left to keep: take it off \`keep\``);
  }
});

test("the exception list is sorted and has no repeats", () => {
  assert.deepEqual(config.files, [...new Set(config.files)].sort());
  assert.deepEqual(config.files.filter((file) => keep.includes(file)), []);
});
