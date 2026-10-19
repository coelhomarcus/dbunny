import { test } from "node:test";
import assert from "node:assert/strict";
import { activeEditor, registerEditor, type EditorHandle } from "../../src/lib/editorBridge.ts";

const handle = (name: string): EditorHandle & { name: string } => ({ name, insert() {}, replaceSelection() {}, hasSelection: () => false });

test("with no editor registered, there is none to write to", () => {
  assert.equal(activeEditor(), null);
});

test("the last editor registered is the one written to", () => {
  const a = handle("a");
  const b = handle("b");
  const offA = registerEditor("d1", a);
  const offB = registerEditor("d2", b);
  assert.equal(activeEditor(), b);
  offB();
  assert.equal(activeEditor(), a);
  offA();
  assert.equal(activeEditor(), null);
});

test("unregistering an older editor keeps the newer one", () => {
  const a = handle("a");
  const b = handle("b");
  const offA = registerEditor("d1", a);
  const offB = registerEditor("d2", b);
  offA();
  assert.equal(activeEditor(), b);
  offB();
});

test("an editor registered again for the same document replaces its old handle", () => {
  const first = handle("first");
  const second = handle("second");
  const offFirst = registerEditor("d1", first);
  const offSecond = registerEditor("d1", second);
  assert.equal(activeEditor(), second);
  // The old unregister does not remove the new one
  offFirst();
  assert.equal(activeEditor(), second);
  offSecond();
  assert.equal(activeEditor(), null);
});
