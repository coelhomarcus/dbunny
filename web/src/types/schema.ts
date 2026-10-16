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

export interface RoleInfo {
  name: string;
  canLogin: boolean;
  superuser: boolean;
  createDb: boolean;
  createRole: boolean;
  replication: boolean;
  bypassRls: boolean;
  /** -1 when unlimited. */
  connectionLimit: number;
  validUntil: string | null;
  memberOf: string[];
  isCurrent: boolean;
  /** Built in (pg_*). */
  isBuiltin: boolean;
}

/** One object in the explorer. */
export interface ExplorerObject {
  name: string;
  kind: "table" | "view" | "matview" | "sequence" | "function" | "procedure" | "type" | "index" | "trigger";
  /** Arguments of a function, a type's kind (enum, domain...), "partitioned" / "foreign" for those tables, or the table an index or trigger belongs to. */
  detail: string;
  /** A partitioned table's partitions. */
  partitions?: ExplorerPartition[];
}

/** One partition, which may itself be partitioned. */
export interface ExplorerPartition {
  schema: string;
  name: string;
  /** The values it holds, e.g. "FOR VALUES FROM ('2026-01-01') TO ('2027-01-01')". */
  bound: string;
  partitions?: ExplorerPartition[];
}

export interface ExplorerSchema {
  name: string;
  objects: ExplorerObject[];
}

export interface ExplorerTree {
  schemas: ExplorerSchema[];
  /** First schema of the search path. */
  defaultSchema: string | null;
}
