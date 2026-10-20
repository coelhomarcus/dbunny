import { test } from "node:test";
import assert from "node:assert/strict";
import { splitShortcut } from "../../src/lib/keys.ts";

test("a shortcut with symbols gives one key for each", () => {
  assert.deepEqual(splitShortcut("⌘↵"), ["⌘", "↵"]);
  assert.deepEqual(splitShortcut("⇧⌘Z"), ["⇧", "⌘", "Z"]);
  assert.deepEqual(splitShortcut("⇧⌥F"), ["⇧", "⌥", "F"]);
  assert.deepEqual(splitShortcut("⌘,"), ["⌘", ","]);
});

test("a shortcut with words keeps each word whole", () => {
  assert.deepEqual(splitShortcut("Ctrl+↵"), ["Ctrl", "↵"]);
  assert.deepEqual(splitShortcut("Shift+Ctrl+Z"), ["Shift", "Ctrl", "Z"]);
  assert.deepEqual(splitShortcut("Ctrl+Shift+Alt+F"), ["Ctrl", "Shift", "Alt", "F"]);
  assert.deepEqual(splitShortcut("Ctrl Enter"), ["Ctrl", "Enter"]);
  assert.deepEqual(splitShortcut("Esc"), ["Esc"]);
  assert.deepEqual(splitShortcut("Ctrl+,"), ["Ctrl", ","]);
});

test("spaces and plus signs only separate", () => {
  assert.deepEqual(splitShortcut("⇧ ⌘ ↵"), ["⇧", "⌘", "↵"]);
  assert.deepEqual(splitShortcut("Ctrl + S"), ["Ctrl", "S"]);
  assert.deepEqual(splitShortcut(""), []);
});

test("a plus that is the key is kept", () => {
  assert.deepEqual(splitShortcut("Ctrl++"), ["Ctrl", "+"]);
});
