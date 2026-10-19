import { test } from "node:test";
import assert from "node:assert/strict";
import { isAssistantShortcut } from "../../src/lib/assistantShortcut.ts";

const key = (over: Partial<{ key: string; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }>) => ({
  key: "i",
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  ...over,
});

test("Cmd+I on a Mac and Ctrl+I elsewhere", () => {
  assert.equal(isAssistantShortcut(key({ metaKey: true }), true), true);
  assert.equal(isAssistantShortcut(key({ ctrlKey: true }), true), false);
  assert.equal(isAssistantShortcut(key({ ctrlKey: true }), false), true);
  assert.equal(isAssistantShortcut(key({ metaKey: true, key: "I" }), true), true);
});

test("other keys and extra modifiers are not the shortcut", () => {
  assert.equal(isAssistantShortcut(key({ metaKey: true, key: "k" }), true), false);
  assert.equal(isAssistantShortcut(key({ metaKey: true, shiftKey: true }), true), false);
  assert.equal(isAssistantShortcut(key({ metaKey: true, altKey: true }), true), false);
  assert.equal(isAssistantShortcut(key({}), true), false);
});
