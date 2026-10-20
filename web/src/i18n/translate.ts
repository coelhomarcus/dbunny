import type { Catalog, Locale, Message, Params, RichNode } from "./messages.ts";

const PLURAL_RULES = new Map<Locale, Intl.PluralRules>();

function rulesOf(locale: Locale): Intl.PluralRules {
  let rules = PLURAL_RULES.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(locale);
    PLURAL_RULES.set(locale, rules);
  }
  return rules;
}

const NUMBER_FORMATS = new Map<Locale, Intl.NumberFormat>();

function numberFormat(locale: Locale): Intl.NumberFormat {
  let format = NUMBER_FORMATS.get(locale);
  if (!format) {
    format = new Intl.NumberFormat(locale);
    NUMBER_FORMATS.set(locale, format);
  }
  return format;
}

/** Replaces `{name}` with `params.name`; a name without a value stays as written. */
export function interpolate(template: string, params?: Params, writeCount?: (count: number) => string): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => {
    if (!(name in params)) return whole;
    const value = params[name];
    // A count is a quantity, so it is written the way the language writes numbers
    return name === "count" && typeof value === "number" && writeCount ? writeCount(value) : String(value);
  });
}

/** The text of a message for a count: a plain string as it is, a plural by the rules of the language. */
export function pick(message: Message, count: number | undefined, locale: Locale): string {
  if (typeof message === "string") return message;
  if (count === undefined) return message.other;
  return message[rulesOf(locale).select(count)] ?? message.other;
}

function isMessage(node: unknown): node is Message {
  return typeof node === "string" || (typeof node === "object" && node !== null && typeof (node as { other?: unknown }).other === "string");
}

/** The message at a dotted key in the first catalog that has it (the catalogs are the fallback chain). */
export function lookup(catalogs: readonly Catalog[], key: string): Message | undefined {
  const path = key.split(".");
  for (const catalog of catalogs) {
    let node: unknown = catalog;
    for (const part of path) {
      node = typeof node === "object" && node !== null ? (node as Record<string, unknown>)[part] : undefined;
    }
    if (isMessage(node)) return node;
  }
  return undefined;
}

/** Looks the key up through the fallback chain and fills in the parameters; a key nobody has comes back as itself. */
export function translate(catalogs: readonly Catalog[], locale: Locale, key: string, params?: Params): string {
  const message = lookup(catalogs, key);
  if (message === undefined) return key;
  const count = typeof params?.count === "number" ? params.count : undefined;
  return interpolate(pick(message, count, locale), params, (n) => numberFormat(locale).format(n));
}

interface Frame {
  tag: string;
  children: RichNode[];
}

/** Joins strings that ended up side by side. */
function merge(nodes: RichNode[]): RichNode[] {
  const out: RichNode[] = [];
  for (const node of nodes) {
    const last = out[out.length - 1];
    if (typeof node === "string" && typeof last === "string") out[out.length - 1] = last + node;
    else if (typeof node === "string") out.push(node);
    else out.push({ tag: node.tag, children: merge(node.children) });
  }
  return out;
}

/** Splits a text with `<tag>…</tag>` into text and nested tags; a tag that doesn't close stays as text. */
export function parseRich(template: string): RichNode[] {
  const stack: Frame[] = [{ tag: "", children: [] }];
  const top = () => stack[stack.length - 1];
  const pattern = /<(\/?)([a-zA-Z][\w-]*)>/g;
  let last = 0;
  for (let match = pattern.exec(template); match; match = pattern.exec(template)) {
    if (match.index > last) top().children.push(template.slice(last, match.index));
    last = pattern.lastIndex;
    const [whole, closing, tag] = match;
    if (!closing) {
      stack.push({ tag, children: [] });
    } else if (stack.length > 1 && top().tag === tag) {
      const done = stack.pop()!;
      top().children.push({ tag: done.tag, children: done.children });
    } else {
      top().children.push(whole);
    }
  }
  if (last < template.length) top().children.push(template.slice(last));
  // Tags that never closed turn back into the text they were written as
  while (stack.length > 1) {
    const open = stack.pop()!;
    top().children.push(`<${open.tag}>`, ...open.children);
  }
  return merge(stack[0].children);
}
