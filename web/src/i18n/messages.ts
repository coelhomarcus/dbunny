/** The languages the app has a translation for. */
export type Locale = "en" | "pt-BR";
/** What a person picks in Settings: one of the languages, or whatever the system uses. */
export type Language = "system" | Locale;

/** Values a message may take: `{name}` in the text is replaced by `params.name`. */
export type Params = Record<string, string | number>;

/** A message that changes with a count; `other` always exists, the rest follow the language's plural rules. */
export interface PluralMessage {
  other: string;
  zero?: string;
  one?: string;
  two?: string;
  few?: string;
  many?: string;
}
export type Message = string | PluralMessage;

/** Messages grouped by area; the leaves are the messages. */
export interface Catalog {
  [key: string]: Message | Catalog;
}

/** The shape another language has to fill: the same keys, a text (or plural forms) in each. */
export type Translation<T> = {
  [K in keyof T]: T[K] extends string ? string : T[K] extends { other: string } ? PluralMessage : Translation<T[K]>;
};

/** Dotted paths to every message of a catalog: "common.cancel". */
export type MessageKey<T, Prefix extends string = ""> = {
  [K in keyof T & string]: T[K] extends string | { other: string } ? `${Prefix}${K}` : MessageKey<T[K], `${Prefix}${K}.`>;
}[keyof T & string];

/** The names in `{braces}` of a text. */
export type ParamsOf<S extends string> = S extends `${string}{${infer P}}${infer Rest}` ? P | ParamsOf<Rest> : never;

/** A text with elements inside: plain strings and tags that wrap more of the same. */
export type RichNode = string | { tag: string; children: RichNode[] };

/** The message at a dotted path of a catalog type. */
export type MessageAt<T, K extends string> = K extends `${infer Head}.${infer Rest}`
  ? Head extends keyof T
    ? MessageAt<T[Head], Rest>
    : never
  : K extends keyof T
    ? T[K]
    : never;

/** The parameters a message needs; a plural always needs `count`. */
export type ParamNames<M> = M extends string ? ParamsOf<M> : M extends { other: infer O extends string } ? ParamsOf<O> | "count" : never;

/** What follows the key in a call: nothing when the message has no parameters. */
export type ArgsFor<M> = [ParamNames<M>] extends [never] ? [params?: Params] : [params: { [N in ParamNames<M>]: string | number }];
