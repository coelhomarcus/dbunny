import type { Engine } from "@/types";

/** The databases offered when creating a connection; the ones not ready yet show as coming soon. */
export const ENGINE_CARDS: { id: "postgres" | "sqlite" | "mysql"; label: string; /** What kind of connection it is, under the name. */ kind: string; available: boolean }[] = [
  { id: "postgres", label: "PostgreSQL", kind: "Server connection", available: true },
  { id: "sqlite", label: "SQLite", kind: "Database file", available: true },
  { id: "mysql", label: "MySQL / MariaDB", kind: "Server connection", available: false },
];

export type EngineCardId = (typeof ENGINE_CARDS)[number]["id"];

/** The name a database engine is shown by, e.g. "PostgreSQL". */
export function engineLabel(engine: Engine): string {
  return ENGINE_CARDS.find((card) => card.id === engine)?.label ?? engine;
}

/** Tooltip of an action the connected engine doesn't support. */
export function notAvailableFor(engine: Engine): string {
  return `Not available for ${engineLabel(engine)}`;
}
