import { useState, useRef, useEffect, useCallback, type Dispatch, type SetStateAction } from "react";
import type { TableDataResponse } from "@/types";
import { resolveEdit, toEditText } from "../lib/cellEdit";

export type PendingChanges = Record<number, Record<string, unknown>>;

interface UseCellEditingOptions {
  data: TableDataResponse | null;
  hasNoPk: boolean;
  pendingChanges: PendingChanges;
  setPendingChanges: Dispatch<SetStateAction<PendingChanges>>;
}

export function useCellEditing({ data, hasNoPk, pendingChanges, setPendingChanges }: UseCellEditingOptions) {
  const [editingCell, setEditingCell] = useState<{ rowIndex: number; colName: string } | null>(null);
  const [editValue, setEditValueState] = useState("");
  // NULL is separate from an empty field: an empty field means an empty string
  const [editIsNull, setEditIsNull] = useState(false);
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
      setEditValueState(toEditText(value));
      setEditIsNull(value === null || value === undefined);
    },
    [hasNoPk, pendingChanges],
  );

  /** Saves `text` (or NULL) as the edited cell's pending value and closes the editor. */
  const commitEdit = useCallback(
    (isNull: boolean, text: string) => {
      if (!editingCell || !data) return;
      const { rowIndex, colName } = editingCell;
      const colIdx = data.columns.findIndex((c) => c.name === colName);
      const originalValue = data.rows[rowIndex][colIdx];
      const outcome = resolveEdit(originalValue, isNull, text);

      setPendingChanges((prev) => {
        const rowChanges = { ...(prev[rowIndex] ?? {}) };
        if (!outcome.changed) {
          delete rowChanges[colName];
        } else {
          rowChanges[colName] = outcome.value;
        }
        if (Object.keys(rowChanges).length === 0) {
          const next = { ...prev };
          delete next[rowIndex];
          return next;
        }
        return { ...prev, [rowIndex]: rowChanges };
      });

      setEditingCell(null);
    },
    [editingCell, data, setPendingChanges],
  );

  const confirmEdit = useCallback(() => commitEdit(editIsNull, editValue), [commitEdit, editIsNull, editValue]);

  const cancelEdit = useCallback(() => setEditingCell(null), []);

  /** Typing replaces NULL with text. */
  const setEditValue = useCallback((value: string) => {
    setEditValueState(value);
    setEditIsNull(false);
  }, []);

  const setEditNull = useCallback(() => {
    setEditValueState("");
    setEditIsNull(true);
  }, []);

  return {
    editingCell,
    editValue,
    editIsNull,
    setEditValue,
    setEditNull,
    inputRef,
    startEdit,
    confirmEdit,
    commitEdit,
    cancelEdit,
  };
}
