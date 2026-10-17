import type { Engine } from "@/types";
import type { SqlDialectName } from "./sqlStatements";

/** The databases offered when creating a connection; the ones not ready yet show as coming soon. */
export const ENGINE_CARDS: { id: "postgres" | "sqlite" | "mysql"; label: string; /** What kind of connection it is, under the name. */ kind: string; available: boolean }[] = [
  { id: "postgres", label: "PostgreSQL", kind: "Server connection", available: true },
  { id: "sqlite", label: "SQLite", kind: "Database file", available: true },
  { id: "mysql", label: "MySQL / MariaDB", kind: "Server connection", available: false },
];

export type EngineCardId = (typeof ENGINE_CARDS)[number]["id"];

/** The name a database engine is shown by, e.g. "PostgreSQL". */
export function engineLabel(engine: EngineCardId): string {
  return ENGINE_CARDS.find((card) => card.id === engine)?.label ?? engine;
}

/** Tooltip of an action the connected engine doesn't support. */
export function notAvailableFor(engine: Engine): string {
  return `Not available for ${engineLabel(engine)}`;
}

/** The SQL a database speaks: it decides how scripts are split, formatted and completed. */
export function sqlDialectOf(engine: Engine | undefined): SqlDialectName {
  return engine === "sqlite" ? "sqlite" : "postgres";
}

/**
 * Tooltip of a structure change the engine can only do by rebuilding the table, which it
 * does not do yet; the others get the plain "not available".
 */
export function needsRebuild(engine: Engine): string {
  return engine === "sqlite" ? "SQLite needs the table rebuilt for this. Coming in a later version." : notAvailableFor(engine);
}

/** What a connection points at: where it is and, for a server, the database on it. */
export interface ConnectionLocation {
  /** `host:port` of a server, the path of a database file. */
  where: string;
  /** The database on the server; null when the location is the database itself (a file). */
  database: string | null;
}

export function locationOf(info: { engine: Engine; host: string; port: number; database: string }): ConnectionLocation {
  if (info.engine === "sqlite") return { where: info.host, database: null };
  return { where: `${info.host}:${info.port}`, database: info.database };
}

/** `host:port/database`, or the path of a file: the hint under a tab's title. */
export function addressOf(info: { engine: Engine; host: string; port: number; database: string }): string {
  const { where, database } = locationOf(info);
  return database === null ? where : `${where}/${database}`;
}
