import dagre from "@dagrejs/dagre";

export const TABLE_WIDTH = 240;
const HEADER_HEIGHT = 32;
const SCHEMA_HEIGHT = 16;
const COLUMN_HEIGHT = 22;
const CARD_PADDING = 8;

/** Vertical center of a column row, matching the card. */
export function columnCenterY(index: number, showSchema: boolean): number {
  const top = showSchema ? HEADER_HEIGHT + SCHEMA_HEIGHT : HEADER_HEIGHT;
  return top + index * COLUMN_HEIGHT + COLUMN_HEIGHT / 2;
}

/** The polyline of a key, leaving the right side of a card and entering the left of another. */
export function relationRoute(sx: number, sy: number, tx: number, ty: number): { x: number; y: number }[] {
  if (tx >= sx + 16) {
    const mid = (sx + tx) / 2;
    return [
      { x: sx, y: sy },
      { x: mid, y: sy },
      { x: mid, y: ty },
      { x: tx, y: ty },
    ];
  }
  const out = 28;
  const midY = Math.abs(ty - sy) < 8 ? sy - 48 : (sy + ty) / 2;
  return [
    { x: sx, y: sy },
    { x: sx + out, y: sy },
    { x: sx + out, y: midY },
    { x: tx - out, y: midY },
    { x: tx - out, y: ty },
    { x: tx, y: ty },
  ];
}

/** Where the extra columns of a composite key are written: the middle of the long segment. */
export function relationLabelAt(points: { x: number; y: number }[]): { x: number; y: number } {
  const from = points[Math.floor((points.length - 1) / 2)];
  const to = points[Math.ceil((points.length - 1) / 2)];
  return { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
}

/** The card's height, matching the node that draws it. */
export function tableHeight(columnCount: number, showSchema: boolean): number {
  return (showSchema ? HEADER_HEIGHT + SCHEMA_HEIGHT : HEADER_HEIGHT) + columnCount * COLUMN_HEIGHT + CARD_PADDING;
}

export interface LayoutNode {
  id: string;
  columnCount: number;
  showSchema: boolean;
}

export interface LayoutEdge {
  source: string;
  target: string;
}

/** Places cards left to right. Positions are the card's top-left corner. */
export function arrangeTables(nodes: LayoutNode[], edges: LayoutEdge[]): Map<string, { x: number; y: number }> {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 80, ranksep: 120, marginx: 40, marginy: 40 });
  for (const node of nodes) {
    graph.setNode(node.id, { width: TABLE_WIDTH, height: tableHeight(node.columnCount, node.showSchema) });
  }
  for (const edge of edges) {
    if (edge.source !== edge.target && graph.hasNode(edge.source) && graph.hasNode(edge.target)) {
      graph.setEdge(edge.source, edge.target);
    }
  }
  dagre.layout(graph);
  const positions = new Map<string, { x: number; y: number }>();
  for (const node of nodes) {
    const placed = graph.node(node.id);
    const height = tableHeight(node.columnCount, node.showSchema);
    positions.set(node.id, { x: placed.x - TABLE_WIDTH / 2, y: placed.y - height / 2 });
  }
  return positions;
}
