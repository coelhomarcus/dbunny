import { useState, useCallback, type Dispatch, type SetStateAction } from "react";
import { type SortingState } from "@tanstack/react-table";
import { useApi } from "../contexts/useConnection";
import type { TableDataResponse, ColumnInfo, TableFilter, FilterMode } from "@/types";
import type { PendingChanges } from "./useCellEditing";
import { t } from "../i18n";
import { errorText } from "../lib/appError";

interface UsePendingChangesOptions {
  data: TableDataResponse | null;
  schema?: string;
  table?: string;
  pkColumns: ColumnInfo[];
  hasNoPk: boolean;
  selectedRows: Set<number>;
  fetchData: (
    s: string,
    t: string,
    pg: number,
    ps: number,
    sort: SortingState,
    filters: TableFilter[],
    filterMode: FilterMode,
  ) => Promise<TableDataResponse>;
  setData: (d: TableDataResponse) => void;
  setSelectedRows: Dispatch<SetStateAction<Set<number>>>;
  page: number;
  pageSize: number;
  sorting: SortingState;
  filters: TableFilter[];
  filterMode: FilterMode;
}

export function usePendingChanges({
  data, schema, table, pkColumns, hasNoPk,
  selectedRows, fetchData, setData, setSelectedRows,
  page, pageSize, sorting, filters, filterMode,
}: UsePendingChangesOptions) {
  const api = useApi();
  const [pendingChanges, setPendingChanges] = useState<PendingChanges>({});
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const dirtyRowCount = Object.keys(pendingChanges).length;
  const hasPendingChanges = dirtyRowCount > 0;

  /** Each changed row with the primary key that finds it, as saving sends them. */
  const changedRows = useCallback(() => {
    if (!data) return [];
    return Object.entries(pendingChanges).map(([rowIdxStr, updates]) => {
      const row = data.rows[parseInt(rowIdxStr)];
      const key: Record<string, unknown> = {};
      for (const pkCol of pkColumns) {
        key[pkCol.name] = row[data.columns.findIndex((c) => c.name === pkCol.name)];
      }
      return { key, updates };
    });
  }, [data, pendingChanges, pkColumns]);

  const handleSave = useCallback(async (): Promise<string | null> => {
    if (!data || !schema || !table || !hasPendingChanges) return null;
    setSaving(true);
    setSaveError(null);

    try {
      // Sent together, in row order, so the server saves all of them or none
      const changes = Object.entries(pendingChanges)
        .sort(([a], [b]) => Number(a) - Number(b))
        .map(([rowIdxStr, updates]) => {
          const row = data.rows[Number(rowIdxStr)];
          const primaryKey: Record<string, unknown> = {};
          for (const pkCol of pkColumns) {
            const colIdx = data.columns.findIndex((c) => c.name === pkCol.name);
            primaryKey[pkCol.name] = row[colIdx];
          }
          return { primaryKey, updates };
        });
      await api.updateRows(schema, table, changes);
      const fresh = await fetchData(schema, table, page, pageSize, sorting, filters, filterMode);
      setData(fresh);
      setPendingChanges({});
      return null;
    } catch (err) {
      const message = errorText(err, t("ui.pendingChanges.saveFailed"));
      setSaveError(message);
      return message;
    } finally {
      setSaving(false);
    }
  }, [data, schema, table, hasPendingChanges, pendingChanges, pkColumns, fetchData, page, pageSize, sorting, filters, filterMode, setData, api]);

  const handleDelete = useCallback(async (): Promise<string | null> => {
    if (!data || !schema || !table || selectedRows.size === 0 || hasNoPk) return null;
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
      const fresh = await fetchData(schema, table, page, pageSize, sorting, filters, filterMode);
      setData(fresh);
      setSelectedRows(new Set());
      setPendingChanges({});
      return null;
    } catch (err) {
      const message = errorText(err, t("ui.pendingChanges.deleteFailed"));
      setSaveError(message);
      return message;
    } finally {
      setDeleting(false);
    }
  }, [data, schema, table, selectedRows, hasNoPk, pkColumns, fetchData, page, pageSize, sorting, filters, filterMode, setData, setSelectedRows, api]);

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
    changedRows,
    discardChanges,
  };
}
