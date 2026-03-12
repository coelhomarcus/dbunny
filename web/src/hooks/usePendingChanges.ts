import { useState, useCallback, type Dispatch, type SetStateAction } from "react";
import { type SortingState } from "@tanstack/react-table";
import { api } from "../lib/api";
import type { TableDataResponse, ColumnInfo } from "@/types";
import type { PendingChanges } from "./useCellEditing";

interface UsePendingChangesOptions {
  data: TableDataResponse | null;
  schema?: string;
  table?: string;
  pkColumns: ColumnInfo[];
  hasNoPk: boolean;
  selectedRows: Set<number>;
  fetchData: (s: string, t: string, pg: number, ps: number, sort: SortingState) => Promise<TableDataResponse>;
  setData: (d: TableDataResponse) => void;
  setSelectedRows: Dispatch<SetStateAction<Set<number>>>;
  page: number;
  pageSize: number;
  sorting: SortingState;
}

export function usePendingChanges({
  data, schema, table, pkColumns, hasNoPk,
  selectedRows, fetchData, setData, setSelectedRows,
  page, pageSize, sorting,
}: UsePendingChangesOptions) {
  const [pendingChanges, setPendingChanges] = useState<PendingChanges>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const dirtyRowCount = Object.keys(pendingChanges).length;
  const hasPendingChanges = dirtyRowCount > 0;

  const handleSave = useCallback(async () => {
    if (!data || !schema || !table || !hasPendingChanges) return;
    setSaving(true);
    setSaveError(null);

    try {
      for (const [rowIdxStr, updates] of Object.entries(pendingChanges)) {
        const rowIndex = parseInt(rowIdxStr);
        const row = data.rows[rowIndex];
        const primaryKey: Record<string, unknown> = {};
        for (const pkCol of pkColumns) {
          const colIdx = data.columns.findIndex((c) => c.name === pkCol.name);
          primaryKey[pkCol.name] = row[colIdx];
        }
        await api.updateRow(schema, table, { primaryKey, updates });
      }
      const fresh = await fetchData(schema, table, page, pageSize, sorting);
      setData(fresh);
      setPendingChanges({});
    } catch (err) {
      setSaveError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  }, [data, schema, table, hasPendingChanges, pendingChanges, pkColumns, fetchData, page, pageSize, sorting, setData]);

  const handleDelete = useCallback(async () => {
    if (!data || !schema || !table || selectedRows.size === 0 || hasNoPk) return;
    setDeleting(true);

    try {
      const primaryKeys = Array.from(selectedRows).map((rowIndex) => {
        const row = data.rows[rowIndex];
        const pk: Record<string, unknown> = {};
        for (const pkCol of pkColumns) {
          const colIdx = data.columns.findIndex((c) => c.name === pkCol.name);
          pk[pkCol.name] = row[colIdx];
        }
        return pk;
      });

      await api.deleteRows(schema, table, primaryKeys);
      const fresh = await fetchData(schema, table, page, pageSize, sorting);
      setData(fresh);
      setSelectedRows(new Set());
      setPendingChanges({});
    } catch (err) {
      setSaveError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to delete");
    } finally {
      setDeleting(false);
    }
  }, [data, schema, table, selectedRows, hasNoPk, pkColumns, fetchData, page, pageSize, sorting, setData, setSelectedRows]);

  const discardChanges = useCallback(() => {
    setPendingChanges({});
    setSaveError(null);
  }, []);

  return {
    pendingChanges,
    setPendingChanges,
    hasPendingChanges,
    dirtyRowCount,
    saving,
    saveError,
    setSaveError,
    deleting,
    handleSave,
    handleDelete,
    discardChanges,
  };
}
