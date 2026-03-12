import { useState, useCallback } from "react";

export function useRowSelection(rowCount: number) {
  const [selectedRows, setSelectedRows] = useState<Set<number>>(new Set());

  const allSelected = selectedRows.size === rowCount && rowCount > 0;
  const someSelected = selectedRows.size > 0 && !allSelected;

  const toggleSelectRow = useCallback((rowIndex: number) => {
    setSelectedRows((prev) => {
      const next = new Set(prev);
      if (next.has(rowIndex)) next.delete(rowIndex);
      else next.add(rowIndex);
      return next;
    });
  }, []);

  const toggleSelectAll = useCallback(() => {
    setSelectedRows(
      allSelected ? new Set() : new Set(Array.from({ length: rowCount }, (_, i) => i)),
    );
  }, [rowCount, allSelected]);

  return {
    selectedRows,
    setSelectedRows,
    allSelected,
    someSelected,
    toggleSelectRow,
    toggleSelectAll,
  };
}
