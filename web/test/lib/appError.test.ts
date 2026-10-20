import { test } from "node:test";
import assert from "node:assert/strict";
import { AppError, errorText, toAppError } from "../../src/lib/appError.ts";
import { activate, loadLocale } from "../../src/i18n/store.ts";
import { hostKeyProblem } from "../../src/lib/sshHostKey.ts";

test("a string rejection becomes an error with the string as its message", () => {
  const error = toAppError("the server said no");
  assert.ok(error instanceof AppError && error instanceof Error);
  assert.equal(error.message, "the server said no");
  assert.equal(error.code, null);
  assert.deepEqual(error.params, {});
});

test("an object from rust keeps its code and params", () => {
  const error = toAppError({ code: "saved.notFound", params: { id: "7" }, message: "Connection with id 7 not found" });
  assert.equal(error.code, "saved.notFound");
  assert.deepEqual(error.params, { id: "7" });
  assert.equal(error.message, "Connection with id 7 not found");
});

test("an object with a null code is a plain message", () => {
  const error = toAppError({ code: null, params: {}, message: "relation \"x\" does not exist" });
  assert.equal(error.code, null);
  assert.equal(error.message, "relation \"x\" does not exist");
});

test("an Error keeps its message and an AppError stays itself", () => {
  assert.equal(toAppError(new Error("boom")).message, "boom");
  const mine = new AppError("x", "a.b", { n: 1 });
  assert.equal(toAppError(mine), mine);
});

test("anything else is written as it is", () => {
  assert.equal(toAppError(undefined).message, "undefined");
  assert.equal(toAppError(42).message, "42");
});

test("errorText says a known code in the language of the app", async () => {
  await loadLocale("pt-BR");
  const error = new AppError("host is required", "connection.hostRequired");
  activate("en");
  assert.equal(errorText(error), "host is required");
  activate("pt-BR");
  assert.equal(errorText(error), "O host é obrigatório");
  activate("en");
});

test("errorText fills the params of the message", async () => {
  await loadLocale("pt-BR");
  const error = new AppError("Connection with id 7 not found", "saved.notFound", { id: "7" });
  activate("pt-BR");
  assert.equal(errorText(error), "Não foi encontrada a conexão com id 7");
  activate("en");
  assert.equal(errorText(error), "Connection with id 7 not found");
});

test("errorText falls back to the message of an unknown code or no code", () => {
  activate("en");
  assert.equal(errorText(new AppError("something else", "no.such.code")), "something else");
  assert.equal(errorText(new AppError("relation x does not exist")), "relation x does not exist");
  assert.equal(errorText("plain text"), "plain text");
  assert.equal(errorText(new Error("an Error")), "an Error");
});

test("errorText uses the fallback when there is nothing to say", () => {
  assert.equal(errorText(undefined, "It didn't work"), "It didn't work");
  assert.equal(errorText(null, "It didn't work"), "It didn't work");
  assert.equal(errorText({}, "It didn't work"), "It didn't work");
  assert.equal(errorText(new AppError(""), "It didn't work"), "It didn't work");
});

test("a host key problem is read from the message of the error", () => {
  const problem = { sshHostKey: { host: "h", fingerprint: "SHA256:x", changed: false } };
  const error = new AppError(JSON.stringify(problem));
  assert.deepEqual(hostKeyProblem(error), problem.sshHostKey);
  assert.deepEqual(hostKeyProblem(JSON.stringify(problem)), problem.sshHostKey);
  assert.equal(hostKeyProblem(new AppError("plain")), null);
  assert.equal(hostKeyProblem(undefined), null);
});
