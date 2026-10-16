/** The databases offered when creating a connection; the ones not ready yet show as coming soon. */
export const ENGINE_CARDS: { id: "postgres" | "sqlite" | "mysql"; label: string; available: boolean }[] = [
  { id: "postgres", label: "PostgreSQL", available: true },
  { id: "sqlite", label: "SQLite", available: false },
  { id: "mysql", label: "MySQL / MariaDB", available: false },
];

export type EngineCardId = (typeof ENGINE_CARDS)[number]["id"];
