import { useSyncExternalStore } from "react";
import { activate, fmt, getLocale, loadLocale, subscribe, t } from "./store.ts";
import { resolveLocale } from "./resolve.ts";
import { updateSettings } from "../lib/settings";
import type { Formatters } from "./format.ts";
import type { Language, Locale } from "./messages.ts";

export { activate, fmt, getLocale, has, loadLocale, raw, subscribe, t } from "./store.ts";
export type { Key, StaticKey } from "./store.ts";
export { LOCALES, isLanguage, resolveLocale } from "./resolve.ts";
export type { Language, Locale, MessageKey } from "./messages.ts";

/** The active language; a component that reads it renders again when the language changes. */
export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, getLocale);
}

/** `t` for components: the screen follows a change of language. */
export function useT(): typeof t {
  useLocale();
  return t;
}

/** Numbers, dates and lists in the active language, for components. */
export function useFormat(): Formatters {
  useLocale();
  return fmt();
}

/** Loads and shows the language the preference points to; called before the first render. */
export async function initI18n(language: Language, systemLanguages: readonly string[]): Promise<Locale> {
  const locale = resolveLocale(language, systemLanguages);
  await loadLocale(locale);
  activate(locale);
  return locale;
}

/** Switches the app to a language and remembers the choice; screens that read `useT` follow at once. */
export async function setLanguage(language: Language): Promise<void> {
  await initI18n(language, navigator.languages);
  updateSettings({ language });
}
