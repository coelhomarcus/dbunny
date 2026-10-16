import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectResponse,
  SchemaInfo,
  TableInfo,
  FunctionInfo,
  FunctionDetail,
  ColumnInfo,
  CompletionSchema,
  FilterMode,
  TableFilter,
  QueryResult,
  TableDataResponse,
  SavedConnection,
  SavedConnectionInput,
} from "@/types";

/** Calls that don't belong to a connection. */
export const api = {
  async connect(connectionUrl: string) {
    return invoke<ConnectResponse>("connect", { connectionUrl });
  },

  async disconnect(sessionId: string) {
    return invoke<boolean>("disconnect", { sessionId });
  },

  async getSavedConnections() {
    return invoke<SavedConnection[]>("get_saved_connections");
  },

  async saveConnection(connection: SavedConnectionInput) {
    return invoke<SavedConnection>("save_connection", { connection });
  },

  async deleteSavedConnection(id: string) {
    return invoke<boolean>("delete_saved_connection", { id });
  },
};

/**
 * Calls bound to one connection. Each window tab has its own session, so pages
 * get this through `useApi()` instead of importing a global.
 */
export function createSessionApi(sessionId: string | null) {
  return {
    async getSchemas() {
      return invoke<SchemaInfo[]>("get_schemas", { sessionId });
    },

    async getTables(schema: string) {
      return invoke<TableInfo[]>("get_tables", { sessionId, schema });
    },

    async getViews(schema: string) {
      return invoke<TableInfo[]>("get_views", { sessionId, schema });
    },

    async getFunctions(schema: string) {
      return invoke<FunctionInfo[]>("get_functions", { sessionId, schema });
    },

    async getFunctionDetail(schema: string, functionName: string, argumentTypes: string) {
      return invoke<FunctionDetail>("get_function_detail", { sessionId, schema, functionName, argumentTypes });
    },

    async getColumns(schema: string, table: string) {
      return invoke<ColumnInfo[]>("get_columns", { sessionId, schema, table });
    },

    async getCompletionSchema() {
      return invoke<CompletionSchema>("get_completion_schema", { sessionId });
    },

    async getTableData(
      schema: string,
      table: string,
      params?: {
        page?: number;
        pageSize?: number;
        sortColumn?: string;
        sortDirection?: "asc" | "desc";
        filters?: TableFilter[];
        filterMode?: FilterMode;
      }
    ) {
      return invoke<TableDataResponse>("get_table_data", {
        sessionId,
        schema,
        table,
        params: params || null,
      });
    },

    /** Exact row count, for tables whose total is only an estimate. */
    async countTableRows(schema: string, table: string) {
      return invoke<number>("count_table_rows", { sessionId, schema, table });
    },

    async query(sql: string) {
      return invoke<QueryResult>("execute_query", { sessionId, sql });
    },

    async updateRow(
      schema: string,
      table: string,
      payload: {
        primaryKey: Record<string, unknown>;
        updates: Record<string, unknown>;
      }
    ) {
      return invoke<{ rowsAffected: number }>("update_row", {
        sessionId,
        schema,
        table,
        primaryKey: payload.primaryKey,
        updates: payload.updates,
      });
    },

    async deleteRows(
      schema: string,
      table: string,
      primaryKeys: Array<Record<string, unknown>>
    ) {
      return invoke<{ rowsDeleted: number }>("delete_rows", {
        sessionId,
        schema,
        table,
        primaryKeys,
      });
    },
  };
}

export type SessionApi = ReturnType<typeof createSessionApi>;
