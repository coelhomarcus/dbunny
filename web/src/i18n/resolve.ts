import type { Language, Locale } from "./messages.ts";

/** Every language the app has a catalog for. */
export const LOCALES: readonly Locale[] = ["en", "pt-BR"];

export function isLanguage(value: unknown): value is Language {
  return value === "system" || LOCALES.some((locale) => locale === value);
}

/** The catalog a system language maps to: all Portuguese reads the Brazilian one, there is no other. */
function localeOf(tag: string): Locale | null {
  const base = tag.toLowerCase().split("-")[0];
  if (base === "pt") return "pt-BR";
  if (base === "en") return "en";
  return null;
}

/** The language to show: the person's pick, or the first system language that has a translation. */
export function resolveLocale(language: Language, systemLanguages: readonly string[]): Locale {
  if (language !== "system") return language;
  for (const tag of systemLanguages) {
    const locale = localeOf(tag);
    if (locale) return locale;
  }
  return "en";
}

/** The language the settings stored; one this version doesn't know (an old file, a newer app) means the system's. */
export function languageFromStored(stored: unknown): Language {
  const language = stored !== null && typeof stored === "object" ? (stored as Record<string, unknown>).language : undefined;
  return isLanguage(language) ? language : "system";
}
