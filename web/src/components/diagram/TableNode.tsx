import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { DiagramColumn } from "@/types";
import { TABLE_WIDTH, tableHeight } from "../../lib/diagramLayout";

export interface TableNodeData {
  schema: string;
  name: string;
  columns: DiagramColumn[];
  showSchema: boolean;
  dimmed: boolean;
  [key: string]: unknown;
}

export type TableNodeType = Node<TableNodeData, "table">;

/** A table card: its name, each column, and which columns are keys. */
export default function TableNode({ data }: NodeProps<TableNodeType>) {
  return (
    <div
      className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-sm"
      style={{ width: TABLE_WIDTH, height: tableHeight(data.columns.length, data.showSchema), opacity: data.dimmed ? 0.35 : 1 }}
    >
      <div className="flex h-8 items-center border-b border-border px-2.5">
        <span className="truncate text-[13px] font-medium">{data.name}</span>
      </div>
      {data.showSchema && (
        <div className="flex h-4 items-center px-2.5 text-[10px] text-muted-foreground">{data.schema}</div>
      )}
      <ul>
        {data.columns.map((column) => (
          <li key={column.name} className="relative flex h-[22px] items-center gap-1.5 px-2">
            <Handle type="target" position={Position.Left} id={column.name} className="!size-1.5 !border-0 !bg-[#71717a]" />
            <span className="w-6 shrink-0 text-[10px] font-medium">
              {column.isPrimaryKey ? <span className="text-warning-foreground">PK</span> : column.isForeignKey ? <span className="text-info-foreground">FK</span> : null}
            </span>
            <span className="min-w-0 flex-1 truncate text-xs">{column.name}</span>
            <span className="max-w-24 shrink-0 truncate text-[10px] text-muted-foreground">{column.typeName}</span>
            <Handle type="source" position={Position.Right} id={column.name} className="!size-1.5 !border-0 !bg-[#71717a]" />
          </li>
        ))}
      </ul>
    </div>
  );
}
