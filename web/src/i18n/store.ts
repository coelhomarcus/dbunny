import { en } from "./locales/en.ts";
import { makeFormatters, type Formatters } from "./format.ts";
import { pseudoCatalog } from "./pseudo.ts";
import { lookup, pick, translate } from "./translate.ts";
import type { ArgsFor, Catalog, Locale, MessageAt, MessageKey, Params, ParamNames } from "./messages.ts";

/** A key of the catalog, for code that picks a message at run time (a table of keys). */
export type Key = MessageKey<typeof en>;

/** The keys whose message has no parameters: what a table of keys can hold and `t` can read without arguments. */
export type StaticKey = { [K in Key]: [ParamNames<MessageAt<typeof en, K>>] extends [never] ? K : never }[Key];

/** Catalogs by language; English is in the main bundle, the others load when they are asked for. */
const LOADERS: Record<Locale, () => Promise<Catalog>> = {
  en: async () => en,
  "pt-BR": async () => (await import("./locales/pt-BR.ts")).ptBR,
};

const loaded = new Map<Locale, Catalog>([["en", en]]);
const listeners = new Set<() => void>();

let locale: Locale = "en";
// The language, then English for what a translation is missing
let chain: Catalog[] = [en];
let formatters: Formatters = makeFormatters("en");
// The pseudo language shows while this is set, whatever `locale` says
let pseudo = false;

export function getLocale(): Locale {
  return locale;
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Fetches the catalog of a language so it can be activated. */
export async function loadLocale(wanted: Locale): Promise<void> {
  if (loaded.has(wanted)) return;
  loaded.set(wanted, await LOADERS[wanted]());
}

/** Makes a loaded language the one the app shows. */
export function activate(next: Locale): void {
  const catalog = loaded.get(next);
  if (!catalog) throw new Error(`The ${next} catalog isn't loaded`);
  if (next === locale && !pseudo) return;
  pseudo = false;
  locale = next;
  chain = next === "en" ? [en] : [catalog, en];
  formatters = makeFormatters(next);
  // `document` doesn't exist where the tests run, and the test project has no DOM types
  const root = (globalThis as { document?: { documentElement: { lang: string } } }).document?.documentElement;
  if (root) root.lang = next;
  listeners.forEach((listener) => listener());
}

/** For development: English made to look foreign and longer, to find text nobody extracted and layouts that don't stretch. Not remembered. */
export function activatePseudo(): void {
  pseudo = true;
  locale = "en";
  chain = [pseudoCatalog(en)];
  formatters = makeFormatters("en");
  const root = (globalThis as { document?: { documentElement: { lang: string } } }).document?.documentElement;
  if (root) root.lang = "en-XA";
  listeners.forEach((listener) => listener());
}

/** A message in the active language. Called while rendering, go through `useT` so the screen follows a change. */
export function t<K extends MessageKey<typeof en>>(key: K, ...args: ArgsFor<MessageAt<typeof en, K>>): string {
  return translate(chain, locale, key, args[0] as Params | undefined);
}

/** A message by a key that is only known at run time (an error code from the backend), with its parameters filled in. */
export function tr(key: string, params?: Params): string {
  return translate(chain, locale, key, params);
}

/** Whether a key that is only known at run time has a message. */
export function has(key: string): boolean {
  return lookup(chain, key) !== undefined;
}

/** The message as written, parameters not yet filled in: what `Rich` reads its tags from. */
export function raw(key: string, count?: number): string {
  const message = lookup(chain, key);
  return message === undefined ? key : pick(message, count, locale);
}

/** Numbers, dates and lists in the active language. */
export function fmt(): Formatters {
  return formatters;
}
