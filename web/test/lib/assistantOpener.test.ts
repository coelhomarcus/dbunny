import { test } from "node:test";
import assert from "node:assert/strict";
import { ConversationOpener } from "../../src/lib/assistantOpener.ts";
import type { OpenedConversation } from "../../src/types/assistant.ts";

const opened = (id: string): OpenedConversation => ({ conversationId: id, acpSessionId: `acp-${id}`, models: null });
const text = (value: string) => ({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: value } });

/** A promise whose end the test decides. */
function gate<T>() {
  let release!: (value: T) => void;
  let fail!: (error: Error) => void;
  const promise = new Promise<T>((resolve, reject) => {
    release = resolve;
    fail = reject;
  });
  return { promise, release, fail };
}

test("what the agent replays while a conversation opens becomes its messages", async () => {
  const opener = new ConversationOpener(() => {});
  const result = await opener.open(async () => {
    opener.receive("c1", text("hello "));
    opener.receive("c1", text("again"));
    return opened("c1");
  });
  assert.equal(result?.opened.conversationId, "c1");
  assert.deepEqual(result?.state.items, [{ kind: "agent", text: "hello again" }]);
});

test("updates are not kept when nothing is opening", () => {
  const opener = new ConversationOpener(() => {});
  assert.equal(opener.receive("c1", text("x")), false);
});

test("an opening that is overtaken is closed and gives nothing", async () => {
  const closed: string[] = [];
  const opener = new ConversationOpener((id) => closed.push(id));
  const slow = gate<OpenedConversation>();
  const first = opener.open(() => slow.promise);
  const second = await opener.open(async () => {
    opener.receive("loaded", text("past message"));
    return opened("loaded");
  });
  // The first, a new chat made ahead, ends last and must not take the place of the loaded one
  slow.release(opened("fresh"));
  assert.equal(await first, null);
  assert.deepEqual(closed, ["fresh"]);
  assert.deepEqual(second?.state.items, [{ kind: "agent", text: "past message" }]);
});

test("the replay of the newer opening survives the older one ending first", async () => {
  const closed: string[] = [];
  const opener = new ConversationOpener((id) => closed.push(id));
  const older = gate<OpenedConversation>();
  const newer = gate<OpenedConversation>();
  const first = opener.open(() => older.promise);
  const second = opener.open(() => newer.promise);
  opener.receive("loaded", text("part one "));
  older.release(opened("fresh"));
  assert.equal(await first, null);
  opener.receive("loaded", text("part two"));
  newer.release(opened("loaded"));
  const result = await second;
  assert.deepEqual(result?.state.items, [{ kind: "agent", text: "part one part two" }]);
  assert.deepEqual(closed, ["fresh"]);
});

test("forgetting while a conversation opens drops it", async () => {
  const closed: string[] = [];
  const opener = new ConversationOpener((id) => closed.push(id));
  const slow = gate<OpenedConversation>();
  const pending = opener.open(() => slow.promise);
  opener.invalidate();
  slow.release(opened("c1"));
  assert.equal(await pending, null);
  assert.deepEqual(closed, ["c1"]);
});

test("an opening that fails says so", async () => {
  const opener = new ConversationOpener(() => {});
  await assert.rejects(
    opener.open(async () => Promise.reject(new Error("no agent"))),
    /no agent/,
  );
});

test("the failure of an opening that was overtaken is not for anyone", async () => {
  const opener = new ConversationOpener(() => {});
  const doomed = gate<OpenedConversation>();
  const pending = opener.open(() => doomed.promise);
  opener.invalidate();
  doomed.fail(new Error("late"));
  assert.equal(await pending, null);
});
