import { useCallback, useEffect, useRef, useState } from "react";
import {
  Background,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  MarkerType,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Edge,
  type Node,
  type OnNodesChange,
} from "@xyflow/react";
import { Download, LocateFixed, Minus, Plus, Search, Waypoints } from "lucide-react";
import "@xyflow/react/dist/style.css";
import type { Diagram } from "@/types";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useApi } from "../../contexts/useConnection";
import { useCapabilities } from "../../hooks/useCapabilities";
import { buildDiagramGraph, type DiagramGraphEdge, type DiagramGraphNode } from "../../lib/diagramGraph";
import { arrangeTables } from "../../lib/diagramLayout";
import { renderDiagramPng } from "../../lib/diagramPng";
import Button from "../ui/Button";
import RelationEdge from "./RelationEdge";
import TableNode, { type TableNodeData } from "./TableNode";

const nodeTypes = { table: TableNode };
const edgeTypes = { relation: RelationEdge };

function edgeOf(edge: DiagramGraphEdge): Edge {
  return {
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: edge.sourceHandle,
    targetHandle: edge.targetHandle,
    type: "relation",
    label: edge.label,
    markerEnd: { type: MarkerType.ArrowClosed, color: "#71717a", width: 16, height: 16 },
  };
}

function nodeOf(
  node: DiagramGraphNode,
  position: { x: number; y: number },
  diagramSchema: string,
  schemas: boolean,
  query: string,
): Node<TableNodeData> {
  const needle = query.trim().toLowerCase();
  const showSchema = schemas || node.schema !== diagramSchema;
  const dimmed = needle !== "" && !node.name.toLowerCase().includes(needle) && !node.schema.toLowerCase().includes(needle);
  return {
    id: node.id,
    type: "table",
    position,
    data: { schema: node.schema, name: node.name, columns: node.columns, showSchema, dimmed },
  };
}

/** Whether the page is in the dark theme, including a change made while it is open. */
function useDark(): boolean {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));
  useEffect(() => {
    const observer = new MutationObserver(() => setDark(document.documentElement.classList.contains("dark")));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  return dark;
}

function failure(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string" && error.message) {
    return error.message;
  }
  return "Couldn't export the diagram";
}

function DiagramCanvas({ schema }: { schema: string }) {
  const api = useApi();
  const capabilities = useCapabilities();
  const { openDocument } = useDocuments();
  const dark = useDark();
  const positions = useRef(new Map<string, { x: number; y: number }>());
  const fittedSchema = useRef<string | null>(null);
  const [graph, setGraph] = useState<{ nodes: DiagramGraphNode[]; edges: DiagramGraphEdge[] } | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [hideUnrelated, setHideUnrelated] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [nodes, setNodes, onNodesChangeBase] = useNodesState<Node<TableNodeData>>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const { fitView, zoomIn, zoomOut } = useReactFlow();

  useEffect(() => {
    let stale = false;
    setLoading(true);
    setLoadError(null);
    api
      .getDiagram(schema)
      .then((diagram: Diagram) => {
        if (stale) return;
        const built = buildDiagramGraph(diagram);
        const placed = arrangeTables(
          built.nodes.map((node) => ({
            id: node.id,
            columnCount: node.columns.length,
            showSchema: capabilities.schemas || node.schema !== schema,
          })),
          built.edges,
        );
        positions.current = placed;
        setGraph(built);
        setLoading(false);
      })
      .catch((e) => {
        if (stale) return;
        setLoadError(typeof e === "string" ? e : "Couldn't read the schema");
        setLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [api, schema, capabilities.schemas]);

  useEffect(() => {
    if (!graph) return;
    const linked = new Set(graph.edges.flatMap((edge) => [edge.source, edge.target]));
    const visible = graph.nodes.filter((node) => !hideUnrelated || linked.has(node.id));
    const ids = new Set(visible.map((node) => node.id));
    setNodes(
      visible.map((node) =>
        nodeOf(node, positions.current.get(node.id) ?? { x: 0, y: 0 }, schema, capabilities.schemas, query),
      ),
    );
    setEdges(graph.edges.filter((edge) => ids.has(edge.source) && ids.has(edge.target)).map(edgeOf));
    if (fittedSchema.current !== schema && visible.length > 0) {
      fittedSchema.current = schema;
      requestAnimationFrame(() => fitView({ padding: 0.2 }));
    }
  }, [graph, hideUnrelated, query, schema, capabilities.schemas, setNodes, setEdges, fitView]);

  const onNodesChange = useCallback<OnNodesChange<Node<TableNodeData>>>(
    (changes) => {
      onNodesChangeBase(changes);
      for (const change of changes) {
        if (change.type === "position" && change.position) positions.current.set(change.id, change.position);
      }
    },
    [onNodesChangeBase],
  );

  function arrange() {
    if (!graph) return;
    const visibleIds = new Set(nodes.map((node) => node.id));
    const visible = graph.nodes.filter((node) => visibleIds.has(node.id));
    const visibleEdges = graph.edges.filter((edge) => visibleIds.has(edge.source) && visibleIds.has(edge.target));
    const placed = arrangeTables(
      visible.map((node) => ({
        id: node.id,
        columnCount: node.columns.length,
        showSchema: capabilities.schemas || node.schema !== schema,
      })),
      visibleEdges,
    );
    for (const [id, position] of placed) positions.current.set(id, position);
    setNodes((current) => current.map((node) => ({ ...node, position: placed.get(node.id) ?? node.position })));
    requestAnimationFrame(() => fitView({ padding: 0.2 }));
  }

  async function exportPng() {
    if (nodes.length === 0) return;
    setExporting(true);
    setExportError(null);
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeFile } = await import("@tauri-apps/plugin-fs");
      const bytes = await renderDiagramPng(
        nodes.map((node) => ({
          id: node.id,
          x: node.position.x,
          y: node.position.y,
          name: node.data.name,
          schema: node.data.schema,
          showSchema: node.data.showSchema,
          columns: node.data.columns,
        })),
        edges.flatMap((edge) =>
          edge.sourceHandle && edge.targetHandle
            ? [
                {
                  source: edge.source,
                  target: edge.target,
                  sourceColumn: edge.sourceHandle,
                  targetColumn: edge.targetHandle,
                  label: typeof edge.label === "string" ? edge.label : "",
                },
              ]
            : [],
        ),
        dark,
      );
      const path = await save({ defaultPath: `${schema}.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
      if (path) await writeFile(path, bytes);
    } catch (e) {
      setExportError(failure(e));
    } finally {
      setExporting(false);
    }
  }

  const empty = !loading && !loadError && graph !== null && graph.nodes.length === 0;

  return (
    <div className="flex h-full min-h-0 flex-col bg-card">
      <header className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2">
        <div className="relative">
          <Search size={13} className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Find a table"
            spellCheck={false}
            className="h-7 w-44 rounded-md bg-muted pl-7 pr-2 text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
          />
        </div>
        <Button variant="ghost-muted" size="xs" aria-pressed={hideUnrelated} onClick={() => setHideUnrelated((v) => !v)}>
          {hideUnrelated ? "Show unrelated" : "Hide unrelated"}
        </Button>
        <span className="mx-1 h-4 w-px bg-border" />
        <Button variant="ghost-muted" size="xs" onClick={arrange} disabled={!graph || graph.nodes.length === 0}>
          <Waypoints /> Arrange
        </Button>
        <Button variant="ghost-muted" size="icon-xs" aria-label="Zoom out" onClick={() => zoomOut()}>
          <Minus />
        </Button>
        <Button variant="ghost-muted" size="icon-xs" aria-label="Zoom in" onClick={() => zoomIn()}>
          <Plus />
        </Button>
        <Button variant="ghost-muted" size="icon-xs" aria-label="Fit" onClick={() => fitView({ padding: 0.2 })}>
          <LocateFixed />
        </Button>
        <Button variant="ghost-muted" size="xs" className="ml-auto" onClick={exportPng} disabled={exporting || !graph || graph.nodes.length === 0}>
          <Download /> {exporting ? "Exporting…" : "Export"}
        </Button>
      </header>
      {exportError && <p className="shrink-0 px-3 py-1 text-xs text-destructive-foreground">{exportError}</p>}
      <div className="min-h-0 flex-1 bg-card">
        {loading ? (
          <p className="flex h-full items-center justify-center text-sm text-muted-foreground">Reading the schema…</p>
        ) : loadError ? (
          <p className="flex h-full items-center justify-center px-6 text-center text-sm text-destructive-foreground">{loadError}</p>
        ) : empty ? (
          <p className="flex h-full items-center justify-center text-sm text-muted-foreground">This schema has no tables.</p>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            onNodeDoubleClick={(_, node: Node<TableNodeData>) =>
              openDocument({ kind: "data", schema: node.data.schema, name: node.data.name, icon: "table" })
            }
            colorMode={dark ? "dark" : "light"}
            fitView
            minZoom={0.15}
            maxZoom={2}
            zoomOnDoubleClick={false}
            nodesConnectable={false}
            proOptions={{ hideAttribution: true }}
          >
            <Background gap={16} color={dark ? "#3f3f46" : "#d4d4d8"} />
            <MiniMap pannable zoomable maskColor={dark ? "rgb(0 0 0 / 0.45)" : "rgb(0 0 0 / 0.08)"} />
          </ReactFlow>
        )}
      </div>
    </div>
  );
}

/** The diagram of one schema. Dragging a card lasts while the tab stays open. */
export default function DiagramPage({ schema }: { schema: string }) {
  return (
    <ReactFlowProvider>
      <DiagramCanvas schema={schema} />
    </ReactFlowProvider>
  );
}
