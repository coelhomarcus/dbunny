export interface SchemaInfo {
  name: string;
}

export interface TableInfo {
  name: string;
  schema: string;
  type: "table" | "view";
}

export interface ColumnInfo {
  name: string;
  dataType: string;
  isNullable: boolean;
  defaultValue: string | null;
  ordinalPosition: number;
  isPrimaryKey: boolean;
}

export interface FunctionInfo {
  name: string;
  schema: string;
  returnType: string;
  argumentTypes: string;
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
