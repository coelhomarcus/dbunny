import { BaseEdge, EdgeLabelRenderer, getSmoothStepPath, type EdgeProps } from "@xyflow/react";

/** A foreign key, drawn into the table it points at. */
export default function RelationEdge({
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  label,
}: EdgeProps) {
  const [path, labelX, labelY] = getSmoothStepPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition });
  const text = typeof label === "string" ? label : "";
  return (
    <>
      <BaseEdge path={path} markerEnd={markerEnd} style={{ stroke: "#71717a" }} />
      {text && (
        <EdgeLabelRenderer>
          <div
            className="nodrag nopan absolute rounded bg-card px-1 text-[10px] text-muted-foreground"
            style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
          >
            {text}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
