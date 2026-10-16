import type { PlanNode, SqlitePlanRow } from "@/types";

export interface PlanRow {
  node: PlanNode;
  depth: number;
  /** "Hash Join", "Seq Scan" */
  title: string;
  /** What it works on: "users u", "using users_pkey" */
  target: string | null;
  /** Conditions and other notable details, one per line. */
  details: string[];
  /** Time spent in the node and its children, across all loops (ANALYZE only). */
  totalMs: number | null;
  /** Time spent in the node itself. */
  selfMs: number | null;
  actualRows: number | null;
  estimatedRows: number;
  /** How far off the row estimate was (10 = ten times), when it's notable. */
  misestimate: number | null;
}

const DETAIL_KEYS = ["Index Cond", "Recheck Cond", "Hash Cond", "Merge Cond", "Join Filter", "Filter", "Sort Key", "Group Key", "Cache Key"];
const MISESTIMATE = 10;

function totalTime(node: PlanNode): number | null {
  if (node["Actual Total Time"] === undefined) return null;
  return node["Actual Total Time"] * (node["Actual Loops"] ?? 1);
}

function details(node: PlanNode): string[] {
  const lines: string[] = [];
  for (const key of DETAIL_KEYS) {
    const value = node[key];
    if (value === undefined) continue;
    lines.push(`${key}: ${Array.isArray(value) ? value.join(", ") : String(value)}`);
  }
  const removed = node["Rows Removed by Filter"] ?? node["Rows Removed by Join Filter"];
  if (typeof removed === "number" && removed > 0) lines.push(`Rows removed by filter: ${removed.toLocaleString()}`);
  if (node["Sort Method"]) lines.push(`Sort: ${String(node["Sort Method"])}, ${String(node["Sort Space Used"])} kB ${String(node["Sort Space Type"] ?? "").toLowerCase()}`.trim());
  const hit = node["Shared Hit Blocks"];
  const read = node["Shared Read Blocks"];
  if (typeof hit === "number" && typeof read === "number" && hit + read > 0) {
    lines.push(`Buffers: ${hit.toLocaleString()} hit, ${read.toLocaleString()} read`);
  }
  return lines;
}

function title(node: PlanNode): string {
  const type = node["Node Type"];
  const join = node["Join Type"];
  // "Hash Join" + "Left" -> "Hash Left Join"
  if (join && join !== "Inner" && type.endsWith("Join")) return type.replace(/Join$/, `${join} Join`);
  if (join && join !== "Inner" && type === "Nested Loop") return `Nested Loop ${join} Join`;
  if (type === "Aggregate" && node["Strategy"]) return `${String(node["Strategy"])} Aggregate`.replace("Plain Aggregate", "Aggregate");
  return type;
}

function target(node: PlanNode): string | null {
  const parts: string[] = [];
  const relation = node["Relation Name"];
  if (relation) parts.push(node.Alias && node.Alias !== relation ? `${relation} ${node.Alias}` : relation);
  else if (node["CTE Name"]) parts.push(`CTE ${String(node["CTE Name"])}`);
  else if (node["Function Name"]) parts.push(`${String(node["Function Name"])}()`);
  if (node["Index Name"]) parts.push(`using ${node["Index Name"]}`);
  return parts.length ? parts.join(" ") : null;
}

/** The plan tree as rows in display order, with times and estimates worked out. */
export function flattenPlan(root: PlanNode): PlanRow[] {
  const rows: PlanRow[] = [];
  const visit = (node: PlanNode, depth: number) => {
    const children = node.Plans ?? [];
    const total = totalTime(node);
    const childTotal = children.reduce((sum, child) => sum + (totalTime(child) ?? 0), 0);
    const loops = node["Actual Loops"] ?? 1;
    const actualRows = node["Actual Rows"] !== undefined ? node["Actual Rows"] * loops : null;
    const estimatedRows = node["Plan Rows"] * loops;
    let misestimate: number | null = null;
    if (actualRows !== null) {
      const ratio = Math.max(actualRows, 1) / Math.max(estimatedRows, 1);
      const off = Math.max(ratio, 1 / ratio);
      if (off >= MISESTIMATE) misestimate = Math.round(off);
    }
    rows.push({
      node,
      depth,
      title: title(node),
      target: target(node),
      details: details(node),
      totalMs: total,
      selfMs: total === null ? null : Math.max(0, total - childTotal),
      actualRows,
      estimatedRows,
      misestimate,
    });
    for (const child of children) visit(child, depth + 1);
  };
  visit(root, 0);
  return rows;
}

export function formatMs(ms: number): string {
  if (ms < 1) return `${ms.toFixed(2)} ms`;
  if (ms < 1000) return `${ms.toFixed(ms < 10 ? 1 : 0)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

/** One line of a SQLite query plan, how deep it sits. */
export interface SqlitePlanLine {
  depth: number;
  detail: string;
}

/**
 * SQLite's EXPLAIN QUERY PLAN as a flat list in reading order: each row sits under the row
 * its `parent` names (the top ones have parent 0).
 */
export function sqlitePlanRows(rows: SqlitePlanRow[]): SqlitePlanLine[] {
  const ids = new Set(rows.map((row) => row.id));
  const children = new Map<number, SqlitePlanRow[]>();
  for (const row of rows) {
    // A row whose parent isn't in the plan is shown at the top
    const parent = ids.has(row.parent) ? row.parent : 0;
    children.set(parent, [...(children.get(parent) ?? []), row]);
  }
  const lines: SqlitePlanLine[] = [];
  const visit = (parent: number, depth: number) => {
    for (const row of children.get(parent) ?? []) {
      lines.push({ depth, detail: row.detail });
      visit(row.id, depth + 1);
    }
  };
  visit(0, 0);
  return lines;
}
