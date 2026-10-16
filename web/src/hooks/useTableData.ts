import { useEffect, useState, useMemo, useCallback } from "react";
import { type SortingState, type OnChangeFn } from "@tanstack/react-table";
import { useApi } from "../contexts/useConnection";
import type { TableDataResponse, ColumnInfo, TableFilter, FilterMode, SortKey } from "@/types";
import { isActiveFilter } from "../lib/tableFilters";

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

/**
 * Rows of one table. The view is remounted for every navigation, so state starts
 * fresh per table; `initialFilters` opens it already filtered (foreign key links).
 */
/** The table's sorting state as the backend's sort keys. */
export function toSortKeys(sorting: SortingState): SortKey[] {
  return sorting.map((s) => ({ column: s.id, direction: s.desc ? "desc" : "asc" }));
}

export function useTableData(schema?: string, table?: string, initialFilters: TableFilter[] = []) {
  const api = useApi();
  const [data, setData] = useState<TableDataResponse | null>(null);
  const [columnInfos, setColumnInfos] = useState<ColumnInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSizeState] = useState(() => readPageSize(schema, table));
  const [sorting, setSorting] = useState<SortingState>([]);
  const [filters, setFilters] = useState<TableFilter[]>(initialFilters);
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [refreshing, setRefreshing] = useState(false);
  const [counting, setCounting] = useState(false);
  const [spinKey, setSpinKey] = useState(0);

  const pkColumns = useMemo(
    () => columnInfos.filter((c) => c.isPrimaryKey),
    [columnInfos],
  );
  const hasNoPk = columnInfos.length > 0 && pkColumns.length === 0;
  const totalPages = data ? Math.ceil(data.totalRows / pageSize) : 0;

  const fetchData = useCallback(
    (
      s: string,
      t: string,
      pg: number,
      ps: number,
      sort: SortingState,
      filterList: TableFilter[] = [],
      mode: FilterMode = "all",
    ) => {
      return api.getTableData(s, t, {
        page: pg,
        pageSize: ps,
        sort: toSortKeys(sort),
        filters: filterList,
        filterMode: mode,
      });
    },
    [api],
  );

  useEffect(() => {
    if (!schema || !table) return;
    setLoading(true);
    setError(null);

    Promise.all([
      fetchData(schema, table, page, pageSize, sorting, filters, filterMode),
      api.getColumns(schema, table),
    ])
      .then(([tableData, cols]) => {
        setData(tableData);
        setColumnInfos(cols);
      })
      .catch((err) => setError(typeof err === "string" ? err : err.message))
      .finally(() => setLoading(false));
  }, [schema, table, page, pageSize, sorting, filters, filterMode, fetchData, api]);

  const setPageSize = useCallback((size: number) => {
    const key = storageKey(schema, table);
    if (key) localStorage.setItem(key, String(size));
    setPageSizeState(size);
    setPage(1);
  }, [schema, table]);

  /** Replaces the estimated total with an exact count. */
  const countExactly = useCallback(async () => {
    if (!schema || !table) return;
    setCounting(true);
    try {
      const total = await api.countTableRows(schema, table);
      setData((prev) => (prev ? { ...prev, totalRows: total, totalRowsEstimated: false } : prev));
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to count rows");
    } finally {
      setCounting(false);
    }
  }, [schema, table, api]);

  /** Applies the filter bar's conditions (incomplete ones are ignored) and goes to page 1. */
  const applyFilters = useCallback((next: TableFilter[], mode: FilterMode) => {
    setFilters(next.filter(isActiveFilter));
    setFilterMode(mode);
    setPage(1);
  }, []);

  const refresh = useCallback(async () => {
    if (!schema || !table) return;
    setRefreshing(true);
    setSpinKey((k) => k + 1);
    try {
      const fresh = await fetchData(schema, table, page, pageSize, sorting, filters, filterMode);
      setData(fresh);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to refresh");
    } finally {
      setRefreshing(false);
    }
  }, [schema, table, page, pageSize, sorting, filters, filterMode, fetchData]);

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
    filters,
    filterMode,
    applyFilters,
    counting,
    countExactly,
    refreshing,
    spinKey,
    refresh,
    fetchData,
    pkColumns,
    hasNoPk,
  };
}
