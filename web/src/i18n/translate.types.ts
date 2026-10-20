// Checked by `tsc -b`, never imported: what the message types promise.
import type { MessageKey, ParamsOf, Translation } from "./messages.ts";

type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Assert<T extends true> = T;

export type ParamsAreRead = Assert<Equal<ParamsOf<"Hi {name}, {n} left">, "name" | "n">>;
export type NoParams = Assert<Equal<ParamsOf<"Save">, never>>;

interface Sample {
  common: { save: "Save" };
  rows: { one: "{count} row"; other: "{count} rows" };
}
export type KeysArePaths = Assert<Equal<MessageKey<Sample>, "common.save" | "rows">>;

// @ts-expect-error a translation has to have every key
export const missingKey: Translation<Sample> = { common: {} };
export const complete: Translation<Sample> = { common: { save: "Salvar" }, rows: { one: "{count} linha", other: "{count} linhas" } };
