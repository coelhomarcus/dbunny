import type { ApprovalRequest } from "../types/assistant";
import { stripContext } from "./assistantContext.ts";

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed";

/** `at` is when it began, in milliseconds, for what happens live; a replayed conversation has none. */
export type Item =
  | { kind: "user"; text: string; at?: number }
  | { kind: "agent"; text: string; at?: number }
  | { kind: "thought"; text: string; at?: number }
  | {
      kind: "tool";
      at?: number;
      /** When it ended, in milliseconds. */
      endedAt?: number;
      id: string;
      /** The tool's name; for the database's own, without the server's prefix. */
      tool: string;
      input: Record<string, unknown>;
      status: ToolStatus;
      output?: string;
      approval?: ApprovalRequest;
    }
  | { kind: "approval"; approval: ApprovalRequest }
  | { kind: "error"; text: string }
  /** A line of what happened to the conversation that is nobody's message: it was stopped, say. */
  | { kind: "note"; text: string };

export interface Conversation {
  items: Item[];
  /** The agent is answering. */
  running: boolean;
  title: string | null;
}

/** The text the assistant is given when the person turns a query down. */
export const DECLINED = "The user declined to run this query. Ask them how to continue.";

const OWN_TOOLS = "mcp__dbunny__";
const OWN_SERVER = "dbunny";

export function emptyConversation(): Conversation {
  return { items: [], running: false, title: null };
}

export type ToolItem = Extract<Item, { kind: "tool" }>;
type Tool = ToolItem;

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

/** The server and the tool of a call that says which MCP server it went to (codex does), if it is the database's. */
function ownCallOf(update: Record<string, unknown>): { tool: string; input: Record<string, unknown> } | null {
  const raw = record(update.rawInput);
  if (raw?.server !== OWN_SERVER) return null;
  const title = typeof update.title === "string" && update.title.startsWith(`mcp.${OWN_SERVER}.`) ? update.title.slice(`mcp.${OWN_SERVER}.`.length) : "";
  const tool = typeof raw.tool === "string" ? raw.tool : title;
  return tool ? { tool, input: record(raw.arguments) ?? {} } : null;
}

function toolNameOf(update: Record<string, unknown>): string | null {
  const name = record(record(update._meta)?.claudeCode)?.toolName;
  if (typeof name === "string") return name.startsWith(OWN_TOOLS) ? name.slice(OWN_TOOLS.length) : name;
  return ownCallOf(update)?.tool ?? null;
}

/** What the call was given: codex wraps the arguments with the server and the tool. */
function inputOf(update: Record<string, unknown>): Record<string, unknown> | null {
  return ownCallOf(update)?.input ?? record(update.rawInput);
}

/** The text a tool returned: in `content` for claude, in `rawOutput.result.content` for codex. */
function textOf(content: unknown, rawOutput?: unknown): string {
  const blocks = Array.isArray(content) ? content : [];
  const text = blocks
    .map((block) => {
      const inner = record(record(block)?.content);
      return inner?.type === "text" && typeof inner.text === "string" ? inner.text : "";
    })
    .join("");
  if (text) return text;
  const raw = record(record(rawOutput)?.result)?.content;
  return (Array.isArray(raw) ? raw : [])
    .map((block) => (record(block)?.type === "text" && typeof record(block)?.text === "string" ? (record(block)?.text as string) : ""))
    .join("");
}

/** The text without the code fence that wraps all of it (the agent puts one around the text of a tool that failed). */
function unfence(text: string): string {
  const lines = text.split("\n");
  const wrapped =
    lines.length >= 3 &&
    /^```\w*$/.test(lines[0].trim()) &&
    lines[lines.length - 1].trim() === "```" &&
    !lines.slice(1, -1).some((line) => line.trim().startsWith("```"));
  return wrapped ? lines.slice(1, -1).join("\n") : text;
}

function statusOf(value: unknown): ToolStatus | null {
  return value === "pending" || value === "in_progress" || value === "completed" || value === "failed" ? value : null;
}

/** Adds text to the last item when it is of this kind, or starts a new one. */
function append(items: Item[], kind: "agent" | "thought" | "user", text: string, now: number | null): Item[] {
  const last = items[items.length - 1];
  if (last && last.kind === kind) return [...items.slice(0, -1), { ...last, text: last.text + text }];
  return [...items, now === null ? { kind, text } : { kind, text, at: now }];
}

function matches(tool: Tool, request: ApprovalRequest): boolean {
  return tool.tool === request.tool && tool.input.sql === request.sql && tool.status !== "completed" && tool.status !== "failed";
}

/** What one `session/update` of the agent adds to the conversation; `now` is when it came, null for a replay. */
export function applyUpdate(conversation: Conversation, raw: unknown, now: number | null = null): Conversation {
  const update = record(raw);
  if (!update) return conversation;
  const kind = update.sessionUpdate;

  if (kind === "agent_message_chunk" || kind === "agent_thought_chunk" || kind === "user_message_chunk") {
    const content = record(update.content);
    if (content?.type !== "text" || typeof content.text !== "string") return conversation;
    const itemKind = kind === "agent_message_chunk" ? "agent" : kind === "agent_thought_chunk" ? "thought" : "user";
    // A thought that has said nothing yet and gets only blanks (agents send breaks between parts) is no Thinking block
    const last = conversation.items[conversation.items.length - 1];
    if (itemKind === "thought" && content.text.trim() === "" && last?.kind !== "thought") return conversation;
    return { ...conversation, items: append(conversation.items, itemKind, content.text, now) };
  }

  if (kind === "session_info_update") {
    return typeof update.title === "string" ? { ...conversation, title: stripContext(update.title) } : conversation;
  }

  if (kind === "tool_call" || kind === "tool_call_update") {
    const id = typeof update.toolCallId === "string" ? update.toolCallId : null;
    if (!id) return conversation;
    const input = inputOf(update);
    const name = toolNameOf(update);
    const status = statusOf(update.status);
    const text = unfence(textOf(update.content, update.rawOutput));
    const at = conversation.items.findIndex((item) => item.kind === "tool" && item.id === id);
    if (at >= 0) {
      const current = conversation.items[at] as Tool;
      const next: Tool = {
        ...current,
        tool: name ?? current.tool,
        input: input ?? current.input,
        status: status ?? current.status,
        output: text || current.output,
        ...(now !== null && (status === "completed" || status === "failed") && current.endedAt === undefined ? { endedAt: now } : {}),
      };
      return { ...conversation, items: conversation.items.map((item, index) => (index === at ? next : item)) };
    }
    const created: Tool = { kind: "tool", id, tool: name ?? "", input: input ?? {}, status: status ?? "pending", ...(text ? { output: text } : {}), ...(now !== null ? { at: now } : {}) };
    // An approval that came before the card takes its place in it
    const waiting = conversation.items.findIndex((item) => item.kind === "approval" && matches(created, item.approval));
    if (waiting >= 0) {
      const standalone = conversation.items[waiting] as Extract<Item, { kind: "approval" }>;
      const items = conversation.items.filter((_, index) => index !== waiting);
      return { ...conversation, items: [...items, { ...created, approval: standalone.approval }] };
    }
    return { ...conversation, items: [...conversation.items, created] };
  }

  return conversation;
}

/** The person sent a message: the agent starts answering. */
export function addUserMessage(conversation: Conversation, text: string): Conversation {
  return { ...conversation, running: true, items: [...conversation.items, { kind: "user", text }] };
}

/** The agent's turn is over, or it failed with `error`; `note` says how it ended when that is worth a line (stopped). */
export function finishTurn(conversation: Conversation, error?: string, note?: string): Conversation {
  const items: Item[] = [...conversation.items];
  if (error) items.push({ kind: "error", text: error });
  if (note) items.push({ kind: "note", text: note });
  return { ...conversation, running: false, items };
}

/** A query waits for the person: it goes on the card that runs it, or on one of its own. */
export function attachApproval(conversation: Conversation, request: ApprovalRequest): Conversation {
  const known = conversation.items.some((item) => (item.kind === "approval" ? item.approval.id : item.kind === "tool" ? item.approval?.id : null) === request.id);
  if (known) return conversation;
  let at = -1;
  conversation.items.forEach((item, index) => {
    if (item.kind === "tool" && !item.approval && matches(item, request)) at = index;
  });
  if (at < 0) return { ...conversation, items: [...conversation.items, { kind: "approval", approval: request }] };
  return { ...conversation, items: conversation.items.map((item, index) => (index === at && item.kind === "tool" ? { ...item, approval: request } : item)) };
}

/** The person answered: nothing is waiting any more. */
export function resolveApproval(conversation: Conversation, approvalId: string): Conversation {
  const items = conversation.items
    .filter((item) => !(item.kind === "approval" && item.approval.id === approvalId))
    .map((item) => {
      if (item.kind === "tool" && item.approval?.id === approvalId) {
        const rest = { ...item };
        delete rest.approval;
        return rest;
      }
      return item;
    });
  return { ...conversation, items };
}

/** One line for what a tool did. */
export function toolLabel(item: Tool): string {
  const text = (key: string) => (typeof item.input[key] === "string" ? (item.input[key] as string) : "");
  switch (item.tool) {
    case "list_tables":
      return text("schema") ? `Listed the tables of ${text("schema")}` : "Listed the tables";
    case "describe_table":
      return `Read the structure of ${text("schema")}.${text("table")}`;
    case "relationships":
      return `Read the relationships of ${text("schema")}`;
    case "run_query":
      return "Ran a query";
    case "explain_query":
      return item.input.analyze === true ? "Explained a query and ran it" : "Explained a query";
    default:
      return item.tool || "Used a tool";
  }
}

export interface ToolRows {
  columns: { name: string; type: string }[];
  rows: unknown[][];
  rowCount: number;
  truncated: boolean;
  valuesHidden: boolean;
}

/** What the agent saw of a query, from the output of its tool; null for any other text. */
export function parseToolOutput(text: string): ToolRows | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const value = record(parsed);
  if (!value || !Array.isArray(value.columns) || typeof value.rowCount !== "number") return null;
  const columns = value.columns.map((column) => {
    const entry = record(column);
    return { name: typeof entry?.name === "string" ? entry.name : "", type: typeof entry?.type === "string" ? entry.type : "" };
  });
  return {
    columns,
    rows: Array.isArray(value.rows) ? (value.rows as unknown[][]) : [],
    rowCount: value.rowCount,
    truncated: value.truncated === true,
    valuesHidden: value.valuesHidden === true,
  };
}
