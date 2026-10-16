export interface SchemaInfo {
  name: string;
}

export interface TableInfo {
  name: string;
  schema: string;
  type: "table" | "view" | "matview";
}

/** A sequence or type listed in the explorer. */
export interface SchemaObject {
  name: string;
  /** For types: "enum", "domain", "composite" or "range". */
  kind: string;
}

export interface ExtensionInfo {
  name: string;
  version: string;
  schema: string;
  comment: string | null;
}

/** Facts and DDL of a sequence or type. */
export interface ObjectDefinition {
  kind: string;
  facts: [string, string][];
  comment: string | null;
  ddl: string;
}

/** A foreign key: `columns` of this table point at `refColumns` of `schema.table`. */
export interface ForeignKeyRef {
  constraint: string;
  schema: string;
  table: string;
  columns: string[];
  refColumns: string[];
}

export interface ColumnInfo {
  name: string;
  /** Standard type name ("USER-DEFINED" for enums, "ARRAY" for arrays). */
  dataType: string;
  /** The type as Postgres prints it: "varchar(80)", "mood", "integer[]". */
  typeName: string;
  isNullable: boolean;
  defaultValue: string | null;
  ordinalPosition: number;
  isPrimaryKey: boolean;
  /** Computed by the server (generated column or identity ALWAYS), so read-only. */
  isGenerated: boolean;
  comment: string | null;
  enumValues: string[] | null;
  foreignKey: ForeignKeyRef | null;
}

export interface FunctionInfo {
  name: string;
  schema: string;
  returnType: string;
  argumentTypes: string;
}

export interface FunctionDetail {
  name: string;
  schema: string;
  returnType: string;
  argumentTypes: string;
  language: string;
  source: string;
  kind: string;
  volatility: string;
  isStrict: boolean;
  owner: string;
  description: string | null;
}

export type SchemaNodeType = "schema" | "table" | "view" | "function" | "folder";

export interface SchemaTreeNode {
  id: string;
  name: string;
  type: SchemaNodeType;
  schema?: string;
  children?: SchemaTreeNode[];
  isLoaded?: boolean;
}

export interface CompletionColumn {
  name: string;
  dataType: string;
}

export interface CompletionRelation {
  schema: string;
  name: string;
  type: "table" | "view";
  columns: CompletionColumn[];
}

export interface CompletionSchema {
  currentSchema: string | null;
  relations: CompletionRelation[];
  functions: FunctionInfo[];
}

/** One server session from pg_stat_activity. */
export interface ServerSession {
  pid: number;
  user: string | null;
  database: string | null;
  application: string | null;
  client: string | null;
  state: string | null;
  waitingFor: string | null;
  querySeconds: number | null;
  transactionSeconds: number | null;
  query: string | null;
  blockedBy: number[];
  /** The connection DBunny used to read this list. */
  isSelf: boolean;
}

export interface ServerActivity {
  sessions: ServerSession[];
  /** Without pg_read_all_stats, other roles' queries are hidden by the server. */
  limitedView: boolean;
}
