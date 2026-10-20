import type { Item, ToolItem } from "./assistantConversation.ts";

/** A span of time as a person says it: 0.4s, 14s, 1m 5s. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${(ms / 1000).toFixed(1)}s`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`;
}

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

function json(text: string | undefined): Record<string, unknown> | null {
  if (!text) return null;
  try {
    const value: unknown = JSON.parse(text);
    return value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/** What a step found, from what the tool gave back. */
function found(item: ToolItem): string {
  if (item.status !== "completed") return "";
  const out = json(item.output);
  if (!out) return "";
  switch (item.tool) {
    case "run_query": {
      if (typeof out.rowCount !== "number") return "";
      const rows = plural(out.rowCount, "row");
      return out.valuesHidden === true ? `${rows}, values hidden` : out.truncated === true ? `${rows} (cut)` : rows;
    }
    case "list_tables":
      return Array.isArray(out.schemas)
        ? plural(out.schemas.reduce((sum: number, schema) => sum + (Array.isArray((schema as { tables?: unknown[] }).tables) ? (schema as { tables: unknown[] }).tables.length : 0), 0), "table")
        : "";
    case "describe_table":
      return Array.isArray(out.columns) ? plural(out.columns.length, "column") : "";
    case "relationships":
      return Array.isArray(out.relations) ? plural(out.relations.length, "relation") : "";
    default:
      return "";
  }
}

/** What is said beside a step: what it found and how long it took, when each is known. */
export function toolDescription(item: ToolItem): string {
  const took = item.at !== undefined && item.endedAt !== undefined && (item.status === "completed" || item.status === "failed") ? formatDuration(item.endedAt - item.at) : "";
  return [found(item), took].filter(Boolean).join(" · ");
}

export type Block =
  | { kind: "item"; item: Item; index: number }
  | {
      kind: "steps";
      items: Item[];
      /** The index of its first item, to key it by. */
      index: number;
      /** It is over: only its summary shows, until it is opened. */
      folded: boolean;
      /** When what comes after it began, to say how long it took. */
      end?: number;
    };

const isStep = (item: Item) => item.kind === "thought" || item.kind === "tool" || item.kind === "approval";

/** Something still going on, or waiting for the person, that must not be hidden. */
function isLive(item: Item): boolean {
  if (item.kind === "approval") return true;
  return item.kind === "tool" && (item.approval !== undefined || item.status === "pending" || item.status === "in_progress");
}

/**
 * The items as the chat draws them: each run of steps (thinking, tools, queries) is one block, folded
 * to its summary once it is over, and the messages between them stand apart.
 */
export function groupItems(items: Item[], running: boolean): Block[] {
  const blocks: Block[] = [];
  let at = 0;
  while (at < items.length) {
    const item = items[at];
    if (!isStep(item)) {
      blocks.push({ kind: "item", item, index: at });
      at++;
      continue;
    }
    const start = at;
    while (at < items.length && isStep(items[at])) at++;
    const steps = items.slice(start, at);
    const next = items[at] as (Item & { at?: number }) | undefined;
    const last = at >= items.length;
    blocks.push({ kind: "steps", items: steps, index: start, folded: !(steps.some(isLive) || (last && running)), end: next?.at });
  }
  return blocks;
}

/** A line for a block of steps: how long they took and how many there were. */
export function stepsSummary(steps: Item[], end: number | undefined): string {
  const first = (steps[0] as Item & { at?: number }).at;
  const finished = steps.reduce<number | undefined>((latest, step) => (step.kind === "tool" && step.endedAt !== undefined ? Math.max(latest ?? 0, step.endedAt) : latest), end);
  const took = first !== undefined && finished !== undefined && finished > first ? formatDuration(finished - first) : null;
  if (steps.every((step) => step.kind === "thought")) return took ? `Thought for ${took}` : "Thought";
  const count = plural(steps.length, "step");
  return took ? `Worked for ${took} · ${count}` : count;
}
