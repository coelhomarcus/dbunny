import { test } from "node:test";
import assert from "node:assert/strict";
import { isLanguage, languageFromStored, LOCALES, resolveLocale } from "../../src/i18n/resolve.ts";

test("an explicit language wins over the system", () => {
  assert.equal(resolveLocale("en", ["pt-BR"]), "en");
  assert.equal(resolveLocale("pt-BR", ["en-US"]), "pt-BR");
});

test("system follows the first language that has a translation", () => {
  assert.equal(resolveLocale("system", ["fr", "pt-BR", "en"]), "pt-BR");
  assert.equal(resolveLocale("system", ["fr", "en-GB", "pt-BR"]), "en");
});

test("any portuguese maps to pt-BR", () => {
  assert.equal(resolveLocale("system", ["pt"]), "pt-BR");
  assert.equal(resolveLocale("system", ["pt-PT"]), "pt-BR");
  assert.equal(resolveLocale("system", ["pt-br"]), "pt-BR");
});

test("english variants map to en", () => {
  assert.equal(resolveLocale("system", ["en-US"]), "en");
  assert.equal(resolveLocale("system", ["EN"]), "en");
});

test("nothing matches means english", () => {
  assert.equal(resolveLocale("system", ["fr", "de"]), "en");
  assert.equal(resolveLocale("system", []), "en");
});

test("isLanguage rejects values it does not know", () => {
  assert.equal(isLanguage("system"), true);
  assert.equal(isLanguage("en"), true);
  assert.equal(isLanguage("pt-BR"), true);
  assert.equal(isLanguage("fr"), false);
  assert.equal(isLanguage(undefined), false);
  assert.equal(isLanguage(42), false);
});

test("the locales are the ones with a catalog", () => {
  assert.deepEqual([...LOCALES], ["en", "pt-BR"]);
});

test("a stored language that no longer exists falls back to system", () => {
  assert.equal(languageFromStored({ language: "fr" }), "system");
  assert.equal(languageFromStored({}), "system");
  assert.equal(languageFromStored(null), "system");
  assert.equal(languageFromStored("pt-BR"), "system");
});

test("a stored valid language is kept", () => {
  assert.equal(languageFromStored({ language: "pt-BR" }), "pt-BR");
  assert.equal(languageFromStored({ language: "en" }), "en");
  assert.equal(languageFromStored({ language: "system" }), "system");
});
