import type { StaticKey } from "../i18n/store.ts";
import type { ColumnGeneration, Engine, ExplainFormat } from "@/types";
import type { SqlDialectName } from "./sqlStatements";

/** The databases offered when creating a connection; one not ready would show as coming soon. */
export const ENGINE_CARDS: { id: "postgres" | "sqlite" | "mysql" | "mariadb"; label: string; /** What kind of connection it is, under the name. */ kindKey: StaticKey; available: boolean }[] = [
  { id: "postgres", label: "PostgreSQL", kindKey: "connect.enginePicker.serverConnection", available: true },
  { id: "sqlite", label: "SQLite", kindKey: "connect.enginePicker.databaseFile", available: true },
  { id: "mysql", label: "MySQL", kindKey: "connect.enginePicker.serverConnection", available: true },
  { id: "mariadb", label: "MariaDB", kindKey: "connect.enginePicker.serverConnection", available: true },
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

/** Whether the engine is one of the servers of the MySQL family (MySQL and MariaDB), which are reached the same way and speak the same SQL. */
export function isMysqlFamily(engine: Engine | undefined): boolean {
  return engine === "mysql" || engine === "mariadb";
}

/** The SQL a database speaks: it decides how scripts are split, formatted and completed. */
export function sqlDialectOf(engine: Engine | undefined): SqlDialectName {
  if (engine === "sqlite") return "sqlite";
  return isMysqlFamily(engine) ? "mysql" : "postgres";
}

/** A column of the table the new-table page starts with. */
export interface StarterColumn {
  name: string;
  dataType: string;
  nullable: boolean;
  primaryKey: boolean;
  default?: string;
  generation?: ColumnGeneration;
}

/** What the table screens offer, which differs between the SQL dialects. Lists, not decisions: what an engine can do is the capabilities' say. */
export interface DialectData {
  /** Types suggested when adding a column to a table. */
  suggestedTypes: string[];
  /** Types suggested on the new-table page. */
  newTableTypes: string[];
  /** The type of a column added with nothing chosen. */
  defaultTextType: string;
  /** The columns a new table starts with. */
  starterColumns: StarterColumn[];
  indexMethods: string[];
  /** The method preselected when creating an index. */
  defaultIndexMethod: string;
  /** How a column added to a table can be produced; empty when the engine can't add such a column. */
  addColumnGenerations: ColumnGeneration[];
  /** How a column of a new table can be produced. */
  newTableGenerations: ColumnGeneration[];
  /** The options of the new-table page that apply: UNLOGGED and PARTITION BY, WITHOUT ROWID and STRICT. */
  createTableFlags: ("unlogged" | "partition" | "withoutRowid" | "strict")[];
}

const POSTGRES_DATA: DialectData = {
  suggestedTypes: ["text", "varchar(255)", "integer", "bigint", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"],
  newTableTypes: ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"],
  defaultTextType: "text",
  starterColumns: [
    { name: "id", dataType: "bigserial", nullable: false, primaryKey: true },
    { name: "created_at", dataType: "timestamptz", nullable: false, primaryKey: false, default: "now()" },
  ],
  indexMethods: ["btree", "hash", "gin", "gist", "brin"],
  defaultIndexMethod: "btree",
  addColumnGenerations: ["none", "identityByDefault", "identityAlways", "generated"],
  newTableGenerations: ["none", "identityByDefault", "identityAlways", "generated"],
  createTableFlags: ["unlogged", "partition"],
};

const SQLITE_DATA: DialectData = {
  suggestedTypes: ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"],
  newTableTypes: ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"],
  defaultTextType: "TEXT",
  starterColumns: [
    { name: "id", dataType: "INTEGER", nullable: false, primaryKey: true },
    { name: "created_at", dataType: "TEXT", nullable: false, primaryKey: false, default: "CURRENT_TIMESTAMP" },
  ],
  indexMethods: [],
  defaultIndexMethod: "btree",
  addColumnGenerations: [],
  newTableGenerations: ["none", "autoincrement", "generated"],
  createTableFlags: ["withoutRowid", "strict"],
};

const MYSQL_TYPES = ["int", "bigint", "varchar(255)", "text", "decimal(12,2)", "tinyint(1)", "date", "datetime", "timestamp", "json", "blob"];

const MYSQL_DATA: DialectData = {
  suggestedTypes: MYSQL_TYPES,
  newTableTypes: MYSQL_TYPES,
  defaultTextType: "varchar(255)",
  starterColumns: [
    { name: "id", dataType: "bigint", nullable: false, primaryKey: true, generation: "autoincrement" },
    { name: "created_at", dataType: "datetime", nullable: false, primaryKey: false, default: "CURRENT_TIMESTAMP" },
  ],
  indexMethods: ["BTREE", "HASH", "FULLTEXT", "SPATIAL"],
  defaultIndexMethod: "BTREE",
  addColumnGenerations: ["none", "autoincrement", "generated"],
  newTableGenerations: ["none", "autoincrement", "generated"],
  createTableFlags: [],
};

export function dialectData(dialect: SqlDialectName): DialectData {
  switch (dialect) {
    case "sqlite":
      return SQLITE_DATA;
    case "mysql":
      return MYSQL_DATA;
    default:
      return POSTGRES_DATA;
  }
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

/** `host:port/database`, or the path of a file: the hint under a tab's title. A MySQL session may have no database. */
export function addressOf(info: { engine: Engine; host: string; port: number; database: string }): string {
  const { where, database } = locationOf(info);
  return database ? `${where}/${database}` : where;
}

/** What the menu says Explain analyze does: MySQL and MariaDB run only statements that read, the others run it and roll it back. */
export function explainAnalyzeHint(format: ExplainFormat): string {
  return format === "mysqlTree" ? "Runs it for real times. Only statements that read" : "Runs it for real times, then rolls it back";
}
