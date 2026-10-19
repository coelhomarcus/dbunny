import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectResponse,
  MysqlConnectParams,
  SshConfig,
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
  StructureChange,
  NewTable,
  ExplainResult,
  ExtensionInfo,
  ObjectDefinition,
  ServerActivity,
  ExplorerTree,
  Diagram,
  DatabaseInfo,
  RoleList,
  AgentStatus,
  ApprovalRequest,
  ConversationOptions,
  Decision,
  OpenedConversation,
  SessionSummary,
} from "@/types";

/** Relation kinds as the explorer names them. */
export type RelationKind = "table" | "view" | "matview";
/** Every kind of relation that has rows, as the server reports it. */
export type RelationType = "table" | "partitioned" | "foreign" | "view" | "matview";

/** Calls that don't belong to a connection. */
export const api = {
  /** Connects, optionally through SSH. `trustHostKey` is an SSH host key fingerprint the user confirmed. */
  async connect(connectionUrl: string, ssh?: SshConfig | null, trustHostKey?: string) {
    return invoke<ConnectResponse>("connect", { connectionUrl, ssh: ssh ?? null, trustHostKey: trustHostKey ?? null });
  },

  /** Connects to a MySQL server, optionally through SSH. `trustHostKey` is an SSH host key fingerprint the user confirmed. */
  async connectMysql(params: MysqlConnectParams, ssh?: SshConfig | null, trustHostKey?: string) {
    return invoke<ConnectResponse>("connect_mysql", { params, ssh: ssh ?? null, trustHostKey: trustHostKey ?? null });
  },

  /** Connects to a MariaDB server, optionally through SSH; the form is the MySQL one. */
  async connectMariadb(params: MysqlConnectParams, ssh?: SshConfig | null, trustHostKey?: string) {
    return invoke<ConnectResponse>("connect_mariadb", { params, ssh: ssh ?? null, trustHostKey: trustHostKey ?? null });
  },

  /** Opens a SQLite file; `create` makes it when it doesn't exist. */
  async connectSqlite(path: string, readOnly: boolean, create: boolean) {
    return invoke<ConnectResponse>("connect_sqlite", { path, readOnly, create });
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

/** The assistant: the agent that answers, and the conversations held with it. */
export const assistantApi = {
  async status() {
    return invoke<AgentStatus>("assistant_status");
  },

  /** Starts the agent unless it is up; `command` is the person's own command, if any. */
  async start(command?: string) {
    return invoke<AgentStatus>("assistant_start", { command: command || null });
  },

  async restart(command?: string) {
    return invoke<AgentStatus>("assistant_restart", { command: command || null });
  },

  async newConversation(sessionId: string, savedId: string | null, options: ConversationOptions) {
    return invoke<OpenedConversation>("assistant_new_conversation", { sessionId, savedId, options });
  },

  async listConversations(sessionId: string, savedId: string | null) {
    return invoke<SessionSummary[]>("assistant_list_conversations", { sessionId, savedId });
  },

  async loadConversation(sessionId: string, savedId: string | null, acpSessionId: string, options: ConversationOptions) {
    return invoke<OpenedConversation>("assistant_load_conversation", { sessionId, savedId, acpSessionId, options });
  },

  /** Sends a message; answers when the agent's turn is over, with why it ended. */
  async prompt(conversationId: string, text: string) {
    return invoke<string>("assistant_prompt", { conversationId, text });
  },

  async cancel(conversationId: string) {
    return invoke<void>("assistant_cancel", { conversationId });
  },

  async approve(approvalId: string, decision: Decision) {
    return invoke<boolean>("assistant_approve", { approvalId, decision });
  },

  async setModel(conversationId: string, modelId: string) {
    return invoke<void>("assistant_set_model", { conversationId, modelId });
  },

  async closeConversation(conversationId: string) {
    return invoke<void>("assistant_close_conversation", { conversationId });
  },

  async log() {
    return invoke<string[]>("assistant_log");
  },

  /** Signs out of Claude on this computer; every conversation ends. */
  async signOut(command?: string) {
    return invoke<void>("assistant_sign_out", { command: command || null });
  },
};

/** An approval request, to refer to by id. */
export type { ApprovalRequest };

/**
 * Calls bound to one connection. Each window tab has its own session, so pages
 * get this through `useApi()` instead of importing a global.
 */
export function createSessionApi(sessionId: string | null) {
  return {
    /** Every schema with its objects, for the explorer. */
    async getExplorer() {
      return invoke<ExplorerTree>("get_explorer", { sessionId });
    },

    /** Tables, columns and foreign keys for the diagram of one schema. */
    async getDiagram(schema: string) {
      return invoke<Diagram>("get_diagram", { sessionId, schema });
    },

    async getExtensions() {
      return invoke<ExtensionInfo[]>("get_extensions", { sessionId });
    },

    /** Facts and DDL of a sequence or a type. */
    async getObjectDefinition(schema: string, name: string, kind: "sequence" | "type" | "index" | "trigger") {
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

    async getRoles() {
      return invoke<RoleList>("get_roles", { sessionId });
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

    async getFunctionDetail(schema: string, functionName: string, argumentTypes: string) {
      return invoke<FunctionDetail>("get_function_detail", { sessionId, schema, functionName, argumentTypes });
    },

    async getColumns(schema: string, table: string) {
      return invoke<ColumnInfo[]>("get_columns", { sessionId, schema, table });
    },
    async getRelationKind(schema: string, name: string) {
      return invoke<RelationType | null>("get_relation_kind", { sessionId, schema, name });
    },

    /** Indexes, constraints, keys, triggers, sizes and DDL of a table or view. */
    async getTableStructure(schema: string, table: string) {
      return invoke<TableStructure>("get_table_structure", { sessionId, schema, table });
    },

    /** The SQL a structure change would run. */
    async createSchema(name: string) {
      return invoke<void>("create_schema", { sessionId, name });
    },

    async renameSchema(name: string, newName: string) {
      return invoke<void>("rename_schema", { sessionId, name, newName });
    },

    async dropSchema(name: string, cascade: boolean) {
      return invoke<void>("drop_schema", { sessionId, name, cascade });
    },

    async createDatabase(name: string) {
      return invoke<void>("create_database", { sessionId, name });
    },

    async dropDatabase(name: string, force: boolean) {
      return invoke<void>("drop_database", { sessionId, name, force });
    },

    async previewStructureChange(schema: string, table: string, change: StructureChange) {
      return invoke<string>("preview_structure_change", { sessionId, schema, table, change });
    },

    async applyStructureChange(schema: string, table: string, change: StructureChange) {
      return invoke<void>("apply_structure_change", { sessionId, schema, table, change });
    },

    async previewCreateTable(schema: string, table: string, spec: NewTable) {
      return invoke<string>("preview_create_table", { sessionId, schema, table, spec });
    },

    async createTable(schema: string, table: string, spec: NewTable) {
      return invoke<void>("create_table", { sessionId, schema, table, spec });
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
    /** Inserts rows (e.g. from a CSV file) in one transaction; nothing is kept if a row fails. */
    async importRows(schema: string, table: string, columns: string[], rows: (string | null)[][]) {
      return invoke<number>("import_rows", { sessionId, schema, table, columns, rows });
    },

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

    /** Saves edited rows in one transaction: all of them, or none if any fails or is gone. */
    async updateRows(
      schema: string,
      table: string,
      changes: { primaryKey: Record<string, unknown>; updates: Record<string, unknown> }[],
    ) {
      return invoke<number>("update_rows", { sessionId, schema, table, changes });
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
