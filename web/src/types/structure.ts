import type { ColumnInfo } from "./schema";

export interface TableMeta {
  kind: "table" | "view" | "materialized view" | "partitioned table" | "foreign table" | string;
  owner: string;
  comment: string | null;
  /** From the statistics; null when the table was never analyzed. */
  rowEstimate: number | null;
  tableBytes: number | null;
  indexesBytes: number | null;
  totalBytes: number | null;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  method: string;
  isUnique: boolean;
  isPrimary: boolean;
  /** Created by a constraint (primary key, unique, exclusion). */
  backsConstraint: boolean;
  /** WHERE clause of a partial index. */
  predicate: string | null;
  definition: string;
  bytes: number | null;
}

export interface ConstraintInfo {
  name: string;
  kind: "primary key" | "unique" | "check" | "foreign key" | "exclusion" | string;
  definition: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  refSchema: string;
  refTable: string;
  refColumns: string[];
  onUpdate: string;
  onDelete: string;
}

export interface ReferencedBy {
  name: string;
  schema: string;
  table: string;
  columns: string[];
}

export interface TriggerInfo {
  name: string;
  timing: string;
  events: string[];
  level: string;
  function: string;
  enabled: boolean;
  definition: string;
}

/** What one role (or PUBLIC) may do on a table. */
export interface TablePrivilege {
  grantee: string;
  privileges: string[];
  /** Privileges it may grant on to others. */
  grantable: string[];
}

export interface TableStructure {
  meta: TableMeta;
  columns: ColumnInfo[];
  indexes: IndexInfo[];
  constraints: ConstraintInfo[];
  foreignKeys: ForeignKeyInfo[];
  referencedBy: ReferencedBy[];
  triggers: TriggerInfo[];
  privileges: TablePrivilege[];
  ddl: string;
  /** Sections that couldn't be read, usually for lack of permission. */
  warnings: string[];
}

/** A change made from the Structure tab; the backend turns it into DDL. */
export type StructureChange =
  | { kind: "addColumn"; name: string; dataType: string; nullable: boolean; default: string | null; comment: string | null; unique: boolean; check: string | null }
  | { kind: "renameColumn"; name: string; newName: string }
  | { kind: "dropColumn"; name: string; cascade: boolean }
  | { kind: "setNullable"; name: string; nullable: boolean }
  | { kind: "setDefault"; name: string; default: string | null }
  | { kind: "setComment"; name: string; comment: string | null }
  | { kind: "createIndex"; name: string | null; columns: string[]; unique: boolean; method: string; concurrently: boolean }
  | { kind: "dropIndex"; name: string; cascade: boolean }
  | { kind: "setType"; name: string; dataType: string; using: string | null }
  | {
      kind: "addForeignKey";
      name: string | null;
      columns: string[];
      refSchema: string;
      refTable: string;
      refColumns: string[];
      onUpdate: string;
      onDelete: string;
    }
  | { kind: "addUnique"; name: string | null; columns: string[] }
  | { kind: "addCheck"; name: string | null; expression: string }
  | { kind: "dropConstraint"; name: string; cascade: boolean }
  | { kind: "setTriggerEnabled"; name: string; enabled: boolean }
  /** `grantee` is a role name, or PUBLIC. */
  | { kind: "grant"; grantee: string; privileges: string[]; grantOption: boolean }
  | { kind: "revoke"; grantee: string; privileges: string[] };

/** One column in the table builder. */
export interface NewColumn {
  name: string;
  dataType: string;
  nullable: boolean;
  primaryKey: boolean;
  default: string | null;
  unique: boolean;
  /** A SQL expression, without `;`. Empty means no check. */
  check: string | null;
  /** PostgreSQL only. Empty means no comment. */
  comment: string | null;
}
