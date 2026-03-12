import { useState, useRef, useEffect, useCallback, type Dispatch, type SetStateAction } from "react";
import type { TableDataResponse } from "@/types";

export type PendingChanges = Record<number, Record<string, unknown>>;

interface UseCellEditingOptions {
  data: TableDataResponse | null;
  hasNoPk: boolean;
  pendingChanges: PendingChanges;
  setPendingChanges: Dispatch<SetStateAction<PendingChanges>>;
}

export function useCellEditing({ data, hasNoPk, pendingChanges, setPendingChanges }: UseCellEditingOptions) {
  const [editingCell, setEditingCell] = useState<{ rowIndex: number; colName: string } | null>(null);
  const [editValue, setEditValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editingCell) inputRef.current?.focus();
  }, [editingCell]);

  const startEdit = useCallback(
    (rowIndex: number, colName: string, originalValue: unknown) => {
      if (hasNoPk) return;
      setEditingCell({ rowIndex, colName });
      const pending = pendingChanges[rowIndex]?.[colName];
      const value = pending !== undefined ? pending : originalValue;
      setEditValue(value === null ? "" : (typeof value === "object" ? JSON.stringify(value) : String(value)));
    },
    [hasNoPk, pendingChanges],
  );

  const confirmEdit = useCallback(() => {
    if (!editingCell || !data) return;
    const { rowIndex, colName } = editingCell;
    const colIdx = data.columns.findIndex((c) => c.name === colName);
    const originalValue = data.rows[rowIndex][colIdx];
    const originalStr = originalValue === null ? "" : (typeof originalValue === "object" ? JSON.stringify(originalValue) : String(originalValue));

    setPendingChanges((prev) => {
      const rowChanges = { ...(prev[rowIndex] ?? {}) };
      if (editValue === originalStr) {
        delete rowChanges[colName];
      } else {
        rowChanges[colName] =
          editValue === "" && originalValue === null ? null : editValue;
      }
      if (Object.keys(rowChanges).length === 0) {
        const next = { ...prev };
        delete next[rowIndex];
        return next;
      }
      return { ...prev, [rowIndex]: rowChanges };
    });

    setEditingCell(null);
  }, [editingCell, editValue, data, setPendingChanges]);

  const cancelEdit = useCallback(() => setEditingCell(null), []);

  return {
    editingCell,
    editValue,
    setEditValue,
    inputRef,
    startEdit,
    confirmEdit,
    cancelEdit,
  };
}
