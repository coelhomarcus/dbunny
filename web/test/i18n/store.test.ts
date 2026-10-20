import { test } from "node:test";
import assert from "node:assert/strict";
import { activate, activatePseudo, getLocale, has, loadLocale, raw, subscribe, t } from "../../src/i18n/store.ts";

test("t answers in the active language", async () => {
  await loadLocale("pt-BR");
  activate("pt-BR");
  assert.equal(t("common.cancel"), "Cancelar");
  activate("en");
  assert.equal(t("common.cancel"), "Cancel");
});

test("listeners are told when the language changes, and only then", async () => {
  await loadLocale("pt-BR");
  activate("en");
  let calls = 0;
  const stop = subscribe(() => calls++);
  activate("pt-BR");
  activate("pt-BR");
  activate("en");
  stop();
  activate("pt-BR");
  assert.equal(calls, 2);
  assert.equal(getLocale(), "pt-BR");
  activate("en");
});

test("switching back to english leaves nothing in portuguese", async () => {
  await loadLocale("pt-BR");
  activate("pt-BR");
  activate("en");
  assert.equal(t("settings.language.label"), "Language");
});

test("a language has to be loaded before it is activated", () => {
  assert.throws(() => activate("fr" as never));
});

test("has tells whether a key exists, raw keeps the parameters in the text", () => {
  activate("en");
  assert.equal(has("common.save"), true);
  assert.equal(has("common.nope"), false);
  assert.equal(raw("common.save"), "Save");
});

test("the pseudo language makes every message look foreign, and english comes back", async () => {
  activate("en");
  activatePseudo();
  assert.ok(t("common.cancel").startsWith("["));
  assert.ok(t("settings.language.label").length > "Language".length);
  assert.equal(getLocale(), "en");
  activate("en");
  assert.equal(t("common.cancel"), "Cancel");
});

test("the pseudo language tells the listeners and keeps parameters working", () => {
  let calls = 0;
  const stop = subscribe(() => calls++);
  activatePseudo();
  stop();
  assert.equal(calls, 1);
  activate("en");
});
