import type { QueryColumn } from "./query";

// Client -> Server
export type WsClientMessage =
  | { type: "query"; sql: string; sessionId: string }
  | { type: "cancel" };

// Server -> Client
export type WsServerMessage =
  | { type: "columns"; columns: QueryColumn[] }
  | { type: "rows"; rows: unknown[][] }
  | { type: "complete"; rowCount: number; duration: number }
  | { type: "error"; message: string };
