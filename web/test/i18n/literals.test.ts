import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { filesWithLooseText } from "./scan.ts";

const ROOT = join(import.meta.dirname, "../..");
const exceptions: string[] = JSON.parse(readFileSync(join(import.meta.dirname, "literalExceptions.json"), "utf8")).files;
const loose = filesWithLooseText(ROOT);

test("no file has loose text unless it is on the exception list", () => {
  const unlisted = [...loose.entries()].filter(([file]) => !exceptions.includes(file));
  const report = unlisted.map(([file, found]) => `${file}:${found[0].line} "${found[0].text}"`);
  assert.deepEqual(report, [], "text in the interface has to go through t(); see web/src/i18n");
});

test("the exception list only holds files that still have loose text", () => {
  const stale = exceptions.filter((file) => !existsSync(join(ROOT, file)) || !loose.has(file));
  assert.deepEqual(stale, [], "these files are clean now: take them off literalExceptions.json");
});

test("the exception list is sorted and has no repeats", () => {
  assert.deepEqual(exceptions, [...new Set(exceptions)].sort());
});
