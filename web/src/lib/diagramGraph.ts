import type { Diagram, DiagramColumn } from "@/types";

/** Stable id for a table card. Schema and name can both contain dots. */
export function tableNodeId(schema: string, name: string): string {
  return `${schema}\u0000${name}`;
}

export interface DiagramGraphNode {
  id: string;
  schema: string;
  name: string;
  columns: DiagramColumn[];
}

export interface DiagramGraphEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle: string;
  targetHandle: string;
  /** Set for a key that covers more than one column: the columns after the first. */
  label: string;
}

/** One node per table and one edge per foreign key. A key whose table is missing is dropped. */
export function buildDiagramGraph(diagram: Diagram): { nodes: DiagramGraphNode[]; edges: DiagramGraphEdge[] } {
  const nodes = diagram.tables.map((table) => ({
    id: tableNodeId(table.schema, table.name),
    schema: table.schema,
    name: table.name,
    columns: table.columns,
  }));
  const ids = new Set(nodes.map((node) => node.id));
  const edges: DiagramGraphEdge[] = [];
  diagram.relations.forEach((relation, index) => {
    const source = tableNodeId(relation.fromSchema, relation.fromTable);
    const target = tableNodeId(relation.toSchema, relation.toTable);
    const from = relation.fromColumns[0];
    const to = relation.toColumns[0];
    if (!from || !to || !ids.has(source) || !ids.has(target)) return;
    const label =
      relation.fromColumns.length > 1
        ? `${relation.fromColumns.slice(1).join(", ")} → ${relation.toColumns.slice(1).join(", ")}`
        : "";
    edges.push({
      id: `${source}:${relation.name}:${index}`,
      source,
      target,
      sourceHandle: from,
      targetHandle: to,
      label,
    });
  });
  return { nodes, edges };
}
