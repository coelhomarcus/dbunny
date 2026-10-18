/** How the plan from EXPLAIN is shaped, so it's drawn the right way. */
export type ExplainFormat = "postgresJson" | "sqliteQueryPlan" | "mysqlTree";

/** What the connected engine supports: screens and actions it lacks are hidden. */
export interface Capabilities {
  // Optional pieces of the driver
  console: boolean;
  transactions: boolean;
  schemaEditing: boolean;
  admin: boolean;
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
  returning: boolean;
  ssh: boolean;
  ssl: boolean;
}
