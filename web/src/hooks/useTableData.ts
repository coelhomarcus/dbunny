import { useEffect, useState, useMemo, useCallback } from "react";
import { type SortingState, type OnChangeFn } from "@tanstack/react-table";
import { api } from "../lib/api";
import type { TableDataResponse, ColumnInfo } from "@/types";

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200];
const DEFAULT_PAGE_SIZE = 50;

function storageKey(schema?: string, table?: string) {
  return schema && table ? `dbunny:pageSize:${schema}.${table}` : null;
}

function readPageSize(schema?: string, table?: string): number {
  const key = storageKey(schema, table);
  if (!key) return DEFAULT_PAGE_SIZE;
  const n = parseInt(localStorage.getItem(key) ?? "", 10);
  return PAGE_SIZE_OPTIONS.includes(n) ? n : DEFAULT_PAGE_SIZE;
}

export function useTableData(schema?: string, table?: string) {
  const [data, setData] = useState<TableDataResponse | null>(null);
  const [columnInfos, setColumnInfos] = useState<ColumnInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSizeState] = useState(() => readPageSize(schema, table));
  const [sorting, setSorting] = useState<SortingState>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [spinKey, setSpinKey] = useState(0);

  const pkColumns = useMemo(
    () => columnInfos.filter((c) => c.isPrimaryKey),
    [columnInfos],
  );
  const hasNoPk = columnInfos.length > 0 && pkColumns.length === 0;
  const totalPages = data ? Math.ceil(data.totalRows / pageSize) : 0;

  const fetchData = useCallback(
    (s: string, t: string, pg: number, ps: number, sort: SortingState) => {
      const sortColumn = sort[0]?.id;
      const sortDirection = sort[0]?.desc
        ? ("desc" as const)
        : ("asc" as const);
      return api.getTableData(s, t, {
        page: pg,
        pageSize: ps,
        sortColumn,
        sortDirection: sortColumn ? sortDirection : undefined,
      });
    },
    [],
  );

  useEffect(() => {
    if (!schema || !table) return;
    setLoading(true);
    setError(null);

    Promise.all([
      fetchData(schema, table, page, pageSize, sorting),
      api.getColumns(schema, table),
    ])
      .then(([tableData, cols]) => {
        setData(tableData);
        setColumnInfos(cols);
      })
      .catch((err) => setError(typeof err === "string" ? err : err.message))
      .finally(() => setLoading(false));
  }, [schema, table, page, pageSize, sorting, fetchData]);

  const setPageSize = useCallback((size: number) => {
    const key = storageKey(schema, table);
    if (key) localStorage.setItem(key, String(size));
    setPageSizeState(size);
    setPage(1);
  }, [schema, table]);

  // Reset state when table changes
  useEffect(() => {
    setPage(1);
    setSorting([]);
    setPageSizeState(readPageSize(schema, table));
  }, [schema, table]);

  const refresh = useCallback(async () => {
    if (!schema || !table) return;
    setRefreshing(true);
    setSpinKey((k) => k + 1);
    try {
      const fresh = await fetchData(schema, table, page, pageSize, sorting);
      setData(fresh);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to refresh");
    } finally {
      setRefreshing(false);
    }
  }, [schema, table, page, pageSize, sorting, fetchData]);

  return {
    data,
    setData,
    columnInfos,
    loading,
    error,
    setError,
    page,
    setPage,
    pageSize,
    setPageSize,
    pageSizeOptions: PAGE_SIZE_OPTIONS,
    totalPages,
    sorting,
    setSorting: setSorting as OnChangeFn<SortingState>,
    refreshing,
    spinKey,
    refresh,
    fetchData,
    pkColumns,
    hasNoPk,
  };
}
