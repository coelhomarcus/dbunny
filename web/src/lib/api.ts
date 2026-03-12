import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectResponse,
  SchemaInfo,
  TableInfo,
  FunctionInfo,
  ColumnInfo,
  QueryResult,
  TableDataResponse,
  SavedConnection,
  SavedConnectionInput,
} from "@/types";

let sessionId: string | null = null;

export function setSessionId(id: string | null) {
  sessionId = id;
}

export function getSessionId() {
  return sessionId;
}

export const api = {
  async connect(connectionUrl: string) {
    return invoke<ConnectResponse>("connect", { connectionUrl });
  },

  async disconnect() {
    return invoke<boolean>("disconnect", { sessionId });
  },

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

  async getColumns(schema: string, table: string) {
    return invoke<ColumnInfo[]>("get_columns", { sessionId, schema, table });
  },

  async getTableData(
    schema: string,
    table: string,
    params?: {
      page?: number;
      pageSize?: number;
      sortColumn?: string;
      sortDirection?: "asc" | "desc";
    }
  ) {
    return invoke<TableDataResponse>("get_table_data", {
      sessionId,
      schema,
      table,
      params: params || null,
    });
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
