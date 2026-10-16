import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectResponse,
  SshConfig,
  SchemaInfo,
  TableInfo,
  FunctionInfo,
  FunctionDetail,
  ColumnInfo,
  CompletionSchema,
  FilterMode,
  TableFilter,
  SortKey,
  QueryResult,
  TableDataResponse,
  SavedConnection,
  SavedConnectionInput,
  TableStructure,
  ExplainResult,
  SchemaObject,
  ExtensionInfo,
  ObjectDefinition,
  ServerActivity,
  DatabaseInfo,
} from "@/types";

/** Relation kinds as the explorer names them. */
export type RelationKind = "table" | "view" | "matview";

/** Calls that don't belong to a connection. */
export const api = {
  /** Connects, optionally through SSH. `trustHostKey` is an SSH host key fingerprint the user confirmed. */
  async connect(connectionUrl: string, ssh?: SshConfig | null, trustHostKey?: string) {
    return invoke<ConnectResponse>("connect", { connectionUrl, ssh: ssh ?? null, trustHostKey: trustHostKey ?? null });
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

    async getMaterializedViews(schema: string) {
      return invoke<TableInfo[]>("get_materialized_views", { sessionId, schema });
    },

    async getSequences(schema: string) {
      return invoke<SchemaObject[]>("get_sequences", { sessionId, schema });
    },

    async getTypes(schema: string) {
      return invoke<SchemaObject[]>("get_types", { sessionId, schema });
    },

    async getExtensions() {
      return invoke<ExtensionInfo[]>("get_extensions", { sessionId });
    },

    /** Facts and DDL of a sequence or a type. */
    async getObjectDefinition(schema: string, name: string, kind: "sequence" | "type") {
      return invoke<ObjectDefinition>("get_object_definition", { sessionId, schema, name, kind });
    },

    async renameRelation(schema: string, name: string, kind: RelationKind, newName: string) {
      return invoke<void>("rename_relation", { sessionId, schema, name, kind, newName });
    },

    async dropRelation(schema: string, name: string, kind: RelationKind, cascade: boolean) {
      return invoke<void>("drop_relation", { sessionId, schema, name, kind, cascade });
    },

    async truncateTable(schema: string, name: string, restartIdentity: boolean, cascade: boolean) {
      return invoke<void>("truncate_table", { sessionId, schema, name, restartIdentity, cascade });
    },

    async refreshMaterializedView(schema: string, name: string) {
      return invoke<void>("refresh_materialized_view", { sessionId, schema, name });
    },

    /** Copies a table's structure (and optionally its rows) under a new name. */
    async duplicateTable(schema: string, name: string, newName: string, withData: boolean) {
      return invoke<void>("duplicate_table", { sessionId, schema, name, newName, withData });
    },

    async getDatabases() {
      return invoke<DatabaseInfo[]>("get_databases", { sessionId });
    },

    /** Opens another database of the same server as a new session, with the same settings. */
    async connectDatabase(database: string) {
      return invoke<ConnectResponse>("connect_database", { sessionId, database });
    },

    async getServerActivity(allDatabases: boolean) {
      return invoke<ServerActivity>("get_server_activity", { sessionId, allDatabases });
    },

    /** Cancels a session's query, or with `terminate` ends the session. */
    async signalBackend(pid: number, terminate: boolean) {
      return invoke<boolean>("signal_backend", { sessionId, pid, terminate });
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

    /** Indexes, constraints, keys, triggers, sizes and DDL of a table or view. */
    async getTableStructure(schema: string, table: string) {
      return invoke<TableStructure>("get_table_structure", { sessionId, schema, table });
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
        sort?: SortKey[];
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

    /** Inserts a row. Columns left out get their defaults; a null value is SQL NULL. */
    async insertRow(schema: string, table: string, values: Record<string, string | null>) {
      return invoke<number>("insert_row", { sessionId, schema, table, values });
    },

    /** Exact row count, for tables whose total is only an estimate. */
    async countTableRows(schema: string, table: string) {
      return invoke<number>("count_table_rows", { sessionId, schema, table });
    },

    async query(sql: string) {
      return invoke<QueryResult>("execute_query", { sessionId, sql });
    },

    /** Stops whatever the query editor is running. */
    async cancelQuery() {
      return invoke<void>("cancel_query", { sessionId });
    },

    /** Opens a transaction that every editor run joins until commit or rollback. */
    async beginTransaction() {
      return invoke<void>("begin_transaction", { sessionId });
    },

    async commitTransaction() {
      return invoke<void>("commit_transaction", { sessionId });
    },

    async rollbackTransaction() {
      return invoke<void>("rollback_transaction", { sessionId });
    },

    async transactionOpen() {
      return invoke<boolean>("transaction_open", { sessionId });
    },

    /** Plan of one statement; with `analyze` it really runs and is rolled back. */
    async explain(sql: string, analyze: boolean) {
      return invoke<ExplainResult>("explain_query", { sessionId, sql, analyze });
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
