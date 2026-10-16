export interface SchemaInfo {
  name: string;
}

export interface TableInfo {
  name: string;
  schema: string;
  type: "table" | "view";
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
