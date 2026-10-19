/** Where the assistant's agent stands. */
export type AgentStatus =
  | { state: "idle" }
  | { state: "starting" }
  | { state: "ready" }
  | { state: "needsNode"; found: string | null }
  | { state: "needsSignIn"; methods: string[] }
  | { state: "unsupported"; message: string }
  | { state: "failed"; message: string }
  | { state: "stopped" };

/** What a conversation is opened with. */
export interface ConversationOptions {
  /** Every query waits for the person's approval. */
  askBeforeQueries: boolean;
  /** The agent sees column names and counts, never values. */
  schemaOnly: boolean;
  /** Rows a query returns to the agent, from 10 to 1000. */
  rowLimit: number;
  /** What the person wrote about this connection. */
  instructions: string;
}

export interface ModelOption {
  id: string;
  name: string;
}

export interface ModelChoice {
  current: string;
  available: ModelOption[];
}

/** A past conversation of a saved connection. */
export interface SessionSummary {
  acpSession: string;
  title: string | null;
  updatedAt: string | null;
}

/** A query waiting for the person's say. */
export interface ApprovalRequest {
  id: string;
  conversationId: string;
  /** `run_query` or `explain_query`. */
  tool: string;
  sql: string;
  analyze: boolean;
}

export type Decision = "once" | "readsInChat" | "decline";

export interface OpenedConversation {
  conversationId: string;
  acpSessionId: string;
  models: ModelChoice | null;
}
