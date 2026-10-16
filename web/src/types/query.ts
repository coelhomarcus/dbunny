export interface QueryRequest {
  sql: string;
}

export interface QueryColumn {
  name: string;
  dataType: string;
}

export interface QueryResult {
  columns: QueryColumn[];
  rows: unknown[][];
  rowCount: number;
  rowsAffected?: number;
  duration: number;
  /** Statements in the script that was run (1 for a plain query). */
  statementCount?: number;
  /** Notices the server sent while it ran (RAISE NOTICE, warnings). */
  notices?: ServerNotice[];
  /** It ran inside the editor's open transaction. */
  inTransaction?: boolean;
}

export interface ServerNotice {
  severity: string;
  message: string;
  detail: string | null;
  hint: string | null;
}

/** One node of Postgres' JSON plan (only the fields DBunny reads are typed). */
export interface PlanNode {
  "Node Type": string;
  "Relation Name"?: string;
  "Alias"?: string;
  "Index Name"?: string;
  "Join Type"?: string;
  "Startup Cost": number;
  "Total Cost": number;
  "Plan Rows": number;
  "Actual Rows"?: number;
  "Actual Loops"?: number;
  "Actual Total Time"?: number;
  Plans?: PlanNode[];
  [key: string]: unknown;
}

export interface ExplainResult {
  plan: [{ Plan: PlanNode; "Planning Time"?: number; "Execution Time"?: number }];
  /** The statement really ran (EXPLAIN ANALYZE) and was rolled back. */
  analyzed: boolean;
}

export interface QueryError {
  message: string;
  position?: number;
  detail?: string;
  hint?: string;
}

/** One condition of the table view's filter bar. */
export interface TableFilter {
  column: string;
  /** SQL operator, see FILTER_OPERATORS */
  operator: string;
  value: string;
}

/** How several filters combine: every one ("all") or at least one ("any"). */
export type FilterMode = "all" | "any";

export interface TableDataRequest {
  page: number;
  pageSize: number;
  sortColumn?: string;
  sortDirection?: "asc" | "desc";
  filters?: Record<string, string>;
}

export interface TableDataResponse {
  columns: QueryColumn[];
  rows: unknown[][];
  totalRows: number;
  /** totalRows comes from the table statistics, not an exact count. */
  totalRowsEstimated?: boolean;
  page: number;
  pageSize: number;
}
