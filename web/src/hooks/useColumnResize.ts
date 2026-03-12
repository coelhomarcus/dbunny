import { useState, useRef } from "react";

const COL_DEFAULT = 180;
const COL_MIN = 60;
const COL_MAX = 800;

export function useColumnResize() {
  const [colWidths, setColWidths] = useState<Record<string, number>>({});
  const resizingCol = useRef<{
    name: string;
    startX: number;
    startWidth: number;
  } | null>(null);

  function getColWidth(name: string) {
    return colWidths[name] ?? COL_DEFAULT;
  }

  function onResizeStart(e: React.MouseEvent, colName: string) {
    e.preventDefault();
    e.stopPropagation();
    resizingCol.current = {
      name: colName,
      startX: e.clientX,
      startWidth: getColWidth(colName),
    };

    function onMouseMove(ev: MouseEvent) {
      if (!resizingCol.current) return;
      const delta = ev.clientX - resizingCol.current.startX;
      const newWidth = Math.max(
        COL_MIN,
        Math.min(COL_MAX, resizingCol.current.startWidth + delta),
      );
      setColWidths((prev) => ({
        ...prev,
        [resizingCol.current!.name]: newWidth,
      }));
    }

    function onMouseUp() {
      resizingCol.current = null;
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    }

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }

  const resetWidths = () => setColWidths({});
  const hasCustomWidths = Object.keys(colWidths).length > 0;

  return { colWidths, getColWidth, onResizeStart, resetWidths, hasCustomWidths };
}
