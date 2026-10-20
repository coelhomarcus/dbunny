import { DECLINED, type Item, type ToolItem } from "./assistantConversation.ts";
import { t } from "../i18n/store.ts";

/** A span of time as a person says it: 0.4s, 14s, 1m 5s. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${(ms / 1000).toFixed(1)}s`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`;
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
      const rows = t("assistant.counts.rows", { count: out.rowCount });
      return out.valuesHidden === true ? t("assistant.found.valuesHidden", { rows }) : out.truncated === true ? t("assistant.found.cut", { rows }) : rows;
    }
    case "list_tables":
      return Array.isArray(out.schemas)
        ? t("assistant.counts.tables", { count: out.schemas.reduce((sum: number, schema) => sum + (Array.isArray((schema as { tables?: unknown[] }).tables) ? (schema as { tables: unknown[] }).tables.length : 0), 0) })
        : "";
    case "describe_table":
      return Array.isArray(out.columns) ? t("assistant.counts.columns", { count: out.columns.length }) : "";
    case "relationships":
      return Array.isArray(out.relations) ? t("assistant.counts.relations", { count: out.relations.length }) : "";
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
  if (steps.every((step) => step.kind === "thought")) return took ? t("assistant.steps.thoughtFor", { took }) : t("assistant.steps.thought");
  const count = t("assistant.counts.steps", { count: steps.length });
  return took ? t("assistant.steps.workedFor", { took, steps: count }) : count;
}

/** The person turned this query down. */
export function isDeclined(item: ToolItem): boolean {
  return item.status === "failed" && item.output === DECLINED;
}

/** A step named for where it stands: in the present while it goes, in the past once done, and plainly when it failed. */
export function toolTitle(item: ToolItem): string {
  const text = (key: string) => (typeof item.input[key] === "string" ? (item.input[key] as string) : "");
  const going = item.status === "pending" || item.status === "in_progress";
  const failed = item.status === "failed";
  const table = `${text("schema")}.${text("table")}`;
  const schema = text("schema");
  switch (item.tool) {
    case "list_tables":
      return t(going ? (schema ? "assistant.tools.listingTablesOf" : "assistant.tools.listingTables") : failed ? (schema ? "assistant.tools.couldntListTablesOf" : "assistant.tools.couldntListTables") : schema ? "assistant.tools.listedTablesOf" : "assistant.tools.listedTables", { schema });
    case "describe_table":
      return t(going ? "assistant.tools.readingStructure" : failed ? "assistant.tools.couldntReadStructure" : "assistant.tools.readStructure", { table });
    case "relationships":
      return t(going ? "assistant.tools.readingRelationships" : failed ? "assistant.tools.couldntReadRelationships" : "assistant.tools.readRelationships", { schema });
    case "run_query":
      return going ? t("assistant.tools.runningQuery") : failed ? (isDeclined(item) ? t("assistant.tools.queryDeclined") : t("assistant.tools.queryFailed")) : t("assistant.tools.ranQuery");
    case "explain_query":
      if (going) return t("assistant.tools.explainingQuery");
      if (failed) return isDeclined(item) ? t("assistant.tools.explainDeclined") : t("assistant.tools.couldntExplain");
      return item.input.analyze === true ? t("assistant.tools.explainedAndRan") : t("assistant.tools.explained");
    default:
      return item.tool || t("assistant.tools.usedTool");
  }
}

/** The assistant was asked and nothing on screen shows it is at work: the chat says so. */
export function isWorking(items: Item[], running: boolean): boolean {
  const last = items[items.length - 1];
  if (!running || !last) return false;
  if (last.kind === "user") return true;
  return last.kind === "tool" && last.approval === undefined && (last.status === "completed" || last.status === "failed");
}
