import type { Catalog, Message } from "./messages.ts";

const ACCENTED: Record<string, string> = { a: "á", e: "é", i: "í", o: "ö", u: "ü", A: "Å", E: "É", I: "Î", O: "Ø", U: "Ů" };

/** English made to look foreign and about a third longer, to find text nobody extracted and layouts that don't stretch. */
export function pseudolocalize(text: string): string {
  // {parameters} and <tags> stay as they are
  const parts = text.split(/(\{\w+\}|<\/?[a-zA-Z][\w-]*>)/);
  let letters = 0;
  const body = parts
    .map((part, index) => {
      if (index % 2 === 1) return part;
      letters += part.length;
      return part.replace(/[aeiouAEIOU]/g, (vowel) => ACCENTED[vowel]);
    })
    .join("");
  return `[${body} ${"·".repeat(Math.max(2, Math.ceil(letters * 0.35)))}]`;
}

function pseudoMessage(message: Message): Message {
  if (typeof message === "string") return pseudolocalize(message);
  return Object.fromEntries(Object.entries(message).map(([form, text]) => [form, pseudolocalize(text)])) as unknown as Message;
}

export function pseudoCatalog(source: Catalog): Catalog {
  const out: Catalog = {};
  for (const [key, value] of Object.entries(source)) {
    const isMessage = typeof value === "string" || typeof (value as { other?: unknown }).other === "string";
    out[key] = isMessage ? pseudoMessage(value as Message) : pseudoCatalog(value as Catalog);
  }
  return out;
}
