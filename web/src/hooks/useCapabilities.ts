import { useConnection } from "../contexts/useConnection";
import type { Capabilities } from "@/types";

/** Without a connection nothing is offered. */
const NONE: Capabilities = {
  console: false,
  transactions: false,
  schemaEditing: false,
  admin: false,
  assistant: false,
  roles: false,
  activity: false,
  schemas: false,
  databases: false,
  materializedViews: false,
  partitions: false,
  functions: false,
  sequences: false,
  types: false,
  extensions: false,
  comments: false,
  alterColumn: false,
  constraints: false,
  triggerToggle: false,
  permissions: false,
  indexMethods: false,
  concurrentIndex: false,
  cascade: false,
  truncateOptions: false,
  duplicateTable: false,
  explainAnalyze: false,
  explainFormat: "postgresJson",
  maintainPrivilege: false,
  virtualGenerated: false,
  sqlFeatures: [],
  ssh: false,
  ssl: false,
};

/** What the connected database supports; screens and actions it lacks are hidden. */
export function useCapabilities(): Capabilities {
  return useConnection().connectionInfo?.capabilities ?? NONE;
}
