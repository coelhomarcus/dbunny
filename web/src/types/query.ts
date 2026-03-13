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
}

export interface QueryError {
  message: string;
  position?: number;
  detail?: string;
  hint?: string;
}

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
  page: number;
  pageSize: number;
}
