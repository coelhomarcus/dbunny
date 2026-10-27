/** How the plan from EXPLAIN is shaped, so it's drawn the right way. */
export type ExplainFormat = "postgresJson" | "sqliteQueryPlan" | "mysqlTree";

/** SQL a server's version speaks, decided by the backend (`sql_features`). */
export type SqlFeature =
  | "returning"
  | "returningUpdate"
  | "onConflict"
  | "onDuplicateKeyUpdate"
  | "nullsOrdering"
  | "ilike"
  | "lateral"
  | "merge"
  | "nullsNotDistinct"
  | "sqlJsonConstructors"
  | "jsonTable"
  | "filterClause"
  | "windowFunctions"
  | "recursiveCte";

/** What the connected engine supports: screens and actions it lacks are hidden. */
export interface Capabilities {
  // Optional pieces of the driver
  console: boolean;
  transactions: boolean;
  schemaEditing: boolean;
  admin: boolean;
  /** The assistant can read this database. */
  assistant: boolean;
  // Server screens
  roles: boolean;
  activity: boolean;
  // Details
  schemas: boolean;
  databases: boolean;
  materializedViews: boolean;
  partitions: boolean;
  functions: boolean;
  sequences: boolean;
  types: boolean;
  extensions: boolean;
  comments: boolean;
  alterColumn: boolean;
  constraints: boolean;
  triggerToggle: boolean;
  permissions: boolean;
  indexMethods: boolean;
  concurrentIndex: boolean;
  cascade: boolean;
  truncateOptions: boolean;
  duplicateTable: boolean;
  explainAnalyze: boolean;
  explainFormat: ExplainFormat;
  /** The MAINTAIN table privilege (PostgreSQL 17). */
  maintainPrivilege: boolean;
  /** Generated columns can be VIRTUAL as well as STORED. */
  virtualGenerated: boolean;
  /** The SQL the server's version speaks, for the editor's suggestions. */
  sqlFeatures: SqlFeature[];
  ssh: boolean;
  ssl: boolean;
}
