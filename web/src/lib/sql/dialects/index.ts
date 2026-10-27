import type { SQLDialect } from "@codemirror/lang-sql";
import type { Engine, SqlFeature } from "@/types";
import { mariadb } from "./mariadb.ts";
import { mysql } from "./mysql.ts";
import { postgres } from "./postgres.ts";
import { sqlite } from "./sqlite.ts";
import type { SqlDialect } from "./types.ts";

export type * from "./types.ts";

const DIALECTS: Record<Engine, SqlDialect> = { postgres, mysql, mariadb, sqlite };

/** The SQL a database speaks: its quoting, words, phrases and functions. */
export function dialectOf(engine: Engine): SqlDialect {
  return DIALECTS[engine];
}

/** The editor's language (highlighting and parsing) for a database. */
export function languageFor(engine: Engine): SQLDialect {
  return DIALECTS[engine].language;
}

/** The items the connected server has: one that needs a feature it lacks is left out. */
export function available<T extends { requires?: SqlFeature }>(items: T[], features: SqlFeature[]): T[] {
  return items.filter((item) => !item.requires || features.includes(item.requires));
}

/** The i18n key of a function's description: `date_trunc` and `DATE_TRUNC` are `dateTrunc`. */
export function functionKey(name: string): string {
  return name.toLowerCase().replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
}
