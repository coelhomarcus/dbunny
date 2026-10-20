import { test } from "node:test";
import assert from "node:assert/strict";
import { agentFromStored, agentInfo, agentSummary, signInMethods } from "../../src/lib/assistantAgents.ts";

test("each agent has its name, company and the folder it keeps conversations in", () => {
  assert.deepEqual(agentInfo("claude"), { kind: "claude", name: "Claude Code", account: "Claude", company: "Anthropic", folder: "~/.claude/projects", loginTitle: "Sign in to Claude", signOutScope: "on this computer, not only in DBunny" });
  assert.deepEqual(agentInfo("codex"), { kind: "codex", name: "Codex", account: "ChatGPT", company: "OpenAI", folder: "DBunny's own Codex folder", loginTitle: "Sign in to Codex", signOutScope: "in DBunny only; your own Codex stays signed in" });
  assert.equal(agentInfo("custom").name, "your agent");
});

test("the agent comes from the setting, or from the command a person had set before there was a choice", () => {
  assert.equal(agentFromStored({ assistantAgent: "codex" }), "codex");
  assert.equal(agentFromStored({ assistantAgent: "claude", assistantCommand: "/a" }), "claude");
  assert.equal(agentFromStored({ assistantAgent: "nonsense", assistantCommand: "/a" }), "custom");
  assert.equal(agentFromStored({ assistantCommand: "/a" }), "custom");
  assert.equal(agentFromStored({ assistantCommand: "  " }), "claude");
  assert.equal(agentFromStored({}), "claude");
  assert.equal(agentFromStored(null), "claude");
});

test("settings say in plain words what an agent uses and where things go", () => {
  assert.equal(
    agentSummary(agentInfo("codex")),
    "Codex uses your ChatGPT account. Your schema and the results you allow go to OpenAI. Chats are kept in DBunny's own Codex folder.",
  );
  assert.match(agentSummary(agentInfo("claude")), /Anthropic.*~\/\.claude\/projects/);
  assert.match(agentSummary(agentInfo("custom")), /its provider/);
});

test("each agent offers the ways of signing in that fit it", () => {
  const methods = [{ id: "chat-gpt", name: "ChatGPT" }, { id: "api-key", name: "API Key" }];
  // Claude signs in from a terminal, so it has no button
  assert.deepEqual(signInMethods("claude", methods), []);
  // Codex: only the browser one; an API key is not something a button can give
  assert.deepEqual(signInMethods("codex", methods), [{ id: "chat-gpt", name: "ChatGPT" }]);
  // An agent of the person's own: every way it announced
  assert.deepEqual(signInMethods("custom", methods), methods);
  assert.deepEqual(signInMethods("custom", []), []);
});

test("settings warn that an agent of the person's own has tools DBunny can't turn off", () => {
  assert.match(agentSummary(agentInfo("custom")), /tools of its own/);
});
