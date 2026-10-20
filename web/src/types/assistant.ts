/** A way the agent offers to sign in. */
export interface AuthMethod {
  id: string;
  name: string;
}

/** Where the assistant's agent stands. */
export type AgentStatus =
  | { state: "idle" }
  /** What it is doing, and the last line the agent wrote while it starts (a download, say). */
  | { state: "starting"; step: "shell" | "node" | "tools" | "agent"; detail: string | null }
  | { state: "ready" }
  | { state: "needsNode"; found: string | null }
  /** `command` is what to type in a terminal to sign in, when the agent is the usual one. */
  | { state: "needsSignIn"; methods: AuthMethod[]; command: string | null }
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
  /** The language of the app, so the agent answers in it. */
  language: string;
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
