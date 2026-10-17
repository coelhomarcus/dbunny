import { useEffect, useMemo, useCallback, useState, useRef } from "react";
import { useLocation, useNavigate, useParams } from "react-router";
import {
  useReactTable,
  getCoreRowModel,
  type ColumnDef,
} from "@tanstack/react-table";
import type { ColumnInfo, QueryColumn, TableFilter } from "@/types";
import { editorFor, toEditText, type CellEditor } from "../lib/cellEdit";
import { isJsonType, shortType } from "../lib/columnMeta";
import CellEditorDialog from "../components/table/CellEditorDialog";
import { useTableData, toSortKeys } from "../hooks/useTableData";
import { useApi, useConnection } from "../contexts/useConnection";
import { useTab } from "../contexts/workspace";
import { useCellEditing } from "../hooks/useCellEditing";
import { useRowSelection } from "../hooks/useRowSelection";
import { useColumnResize } from "../hooks/useColumnResize";
import { useColumnLayout } from "../hooks/useColumnLayout";
import ColumnsMenu from "../components/table/ColumnsMenu";
import TableTools from "../components/table/TableTools";
import { usePendingChanges } from "../hooks/usePendingChanges";
import { useDiscardGuard } from "../hooks/useDiscardGuard";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import FilterBar from "../components/table/FilterBar";
import ImportCsvDialog from "../components/table/ImportCsvDialog";
import Button from "../components/ui/Button";
import { ArrowUpRight, Ban, Braces, Copy, CopyPlus, Database, Filter, Plus, Sheet, TriangleAlert } from "lucide-react";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu";
import { pendingChangesToSql, rowsToInsertSql, rowsToJson, rowsToTsv, valueText } from "../lib/copyRows";
import SqlViewer from "../components/SqlViewer";
import Dialog from "../components/ui/Dialog";
import Checkbox from "../components/Checkbox";
import TableViewHeader from "../components/TableViewHeader";
import { writeClipboardText } from "../lib/clipboard";
import SortableColumnHeader from "../components/SortableColumnHeader";
import EditableCell from "../components/EditableCell";
import Pagination from "../components/Pagination";
import { TableSkeleton } from "../components/Skeleton";
import NoticeToast, { type Notice } from "../components/ui/NoticeToast";

/** Where a foreign key cell leads: the referenced table, filtered to the matching row. */
function foreignKeyLink(column: ColumnInfo | undefined, row: unknown[], columns: QueryColumn[]) {
  const fk = column?.foreignKey;
  if (!fk) return null;
  const filters: TableFilter[] = [];
  for (const [i, name] of fk.columns.entries()) {
    const value = row[columns.findIndex((c) => c.name === name)];
    // A key with a NULL part references nothing
    if (value === null || value === undefined) return null;
    filters.push({ column: fk.refColumns[i], operator: "=", value: toEditText(value) });
  }
  return { path: `/db/${encodeURIComponent(fk.schema)}/${encodeURIComponent(fk.table)}`, filters };
}

/**
 * Every navigation (sidebar, foreign key link) gets a fresh view, opened with the
 * filters it was given, so nothing from the previous table carries over.
 */
export default function TableView() {
  const location = useLocation();
  const state = location.state as { filters?: TableFilter[] } | null;
  return <TableViewContent key={location.key} initialFilters={state?.filters} />;
}

function TableViewContent({ initialFilters }: { initialFilters?: TableFilter[] }) {
  const { schema, table } = useParams<{ schema: string; table: string }>();
  const api = useApi();
  const navigate = useNavigate();
  // Hidden window tabs stay mounted; only the visible one reacts to shortcuts
  const { isActive } = useTab();

  const tableData = useTableData(schema, table, initialFilters);
  const { data, columnInfos, loading, error, hasNoPk } = tableData;
  const connectionReadOnly = useConnection().connectionInfo?.readOnly ?? false;
  const { relationType } = tableData;
  // A view's rows come from its query: nothing to insert into or edit here
  const isView = relationType === "view" || relationType === "matview";
  const readOnly = connectionReadOnly || isView;
  // Rows can't be changed without a primary key to find them, or when nothing can be written
  const editLocked = hasNoPk || readOnly;

  const selection = useRowSelection(data?.rows.length ?? 0);
  const { connectionInfo } = useConnection();
  const layout = useColumnLayout(
    connectionInfo && schema && table
      ? `dbunny:columns:${connectionInfo.host}:${connectionInfo.port}/${connectionInfo.database}/${schema}.${table}`
      : null,
    data?.columns,
  );
  const resize = useColumnResize();

  const pending = usePendingChanges({
    data,
    schema,
    table,
    pkColumns: tableData.pkColumns,
    hasNoPk,
    selectedRows: selection.selectedRows,
    fetchData: tableData.fetchData,
    setData: tableData.setData,
    setSelectedRows: selection.setSelectedRows,
    page: tableData.page,
    pageSize: tableData.pageSize,
    sorting: tableData.sorting,
    filters: tableData.filters,
    filterMode: tableData.filterMode,
  });

  const [filtersOpen, setFiltersOpen] = useState(() => (initialFilters?.length ?? 0) > 0);
  const [selectedColumn, setSelectedColumn] = useState<string | null>(null);
  useEffect(() => {
    if (!selectedColumn) return;
    const column = selectedColumn;
    function onPointerDown(e: PointerEvent) {
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest(`[data-grid-column="${CSS.escape(column)}"]`)) return;
      setSelectedColumn(null);
    }
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [selectedColumn]);
  // A new row edited in the grid. Missing keys are left to the column default.
  const [draft, setDraft] = useState<Record<string, string | null> | null>(null);
  const [draftCol, setDraftCol] = useState<string | null>(null);
  const [draftText, setDraftText] = useState("");
  const [draftIsNull, setDraftIsNull] = useState(true);
  const [draftExpanded, setDraftExpanded] = useState(false);
  const draftInputRef = useRef<HTMLInputElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; rowIndex: number; colName: string } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [reviewSql, setReviewSql] = useState<string | null>(null);
  // Open with no text: choose a file; with text: rows pasted into the table
  const [importOpen, setImportOpen] = useState<{ text?: string } | false>(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const dismissNotice = useCallback(() => setNotice(null), []);
  const [sqlCopied, setSqlCopied] = useState(false);

  const editing = useCellEditing({
    data,
    hasNoPk: editLocked,
    pendingChanges: pending.pendingChanges,
    setPendingChanges: pending.setPendingChanges,
  });

  // The cell whose inline edit moved to the dialog editor (compared by identity, so a new edit starts inline)
  const [expandedCell, setExpandedCell] = useState<typeof editing.editingCell>(null);
  const editingCell = editing.editingCell;
  const editingColumn = editingCell ? columnInfos.find((c) => c.name === editingCell.colName) : undefined;
  const editingEditor: CellEditor | null = (() => {
    if (!editingCell || !data) return null;
    if (expandedCell === editingCell) return { kind: "long", json: isJsonType(editingColumn) };
    const colIdx = data.columns.findIndex((c) => c.name === editingCell.colName);
    const pendingValue = pending.pendingChanges[editingCell.rowIndex]?.[editingCell.colName];
    return editorFor(editingColumn, pendingValue !== undefined ? pendingValue : data.rows[editingCell.rowIndex]?.[colIdx]);
  })();
  const editorDialogOpen = editingEditor?.kind === "long";

  // Loading other rows (page, sorting, refresh) invalidates edits and selection by position
  const { discardChanges } = pending;
  const { cancelEdit } = editing;
  const { setSelectedRows } = selection;
  const resetRowState = useCallback(() => {
    discardChanges();
    cancelEdit();
    setSelectedRows(new Set());
    setDraft(null);
    setDraftCol(null);
    setDraftExpanded(false);
  }, [discardChanges, cancelEdit, setSelectedRows]);
  const discardGuard = useDiscardGuard(pending.dirtyRowCount + (draft ? 1 : 0), resetRowState);
  const { guard } = discardGuard;

  const openDraft = useCallback((initial: Record<string, string | null> = {}) => {
    setDraft(initial);
    setDraftCol(null);
    setDraftExpanded(false);
    requestAnimationFrame(() => gridRef.current?.scrollTo({ top: 0, behavior: "smooth" }));
  }, []);

  const commitDraft = useCallback((name: string, isNull: boolean, text: string) => {
    setDraft((prev) => {
      if (!prev) return prev;
      if (!(name in prev) && isNull && text === "") return prev;
      return { ...prev, [name]: isNull ? null : text };
    });
    setDraftCol(null);
    setDraftExpanded(false);
  }, []);

  const saveAll = useCallback(async () => {
    if (editing.editingCell) editing.confirmEdit();
    let values = draft;
    if (draft && draftCol) {
      values = !(draftCol in draft) && draftIsNull && draftText === "" ? draft : { ...draft, [draftCol]: draftIsNull ? null : draftText };
      setDraft(values);
      setDraftCol(null);
      setDraftExpanded(false);
    }
    const hadEdits = pending.hasPendingChanges;
    if (hadEdits) {
      const err = await pending.handleSave();
      if (err) {
        setNotice({ tone: "error", text: err });
        return;
      }
    }
    if (values) {
      try {
        await api.insertRow(schema!, table!, values);
        setDraft(null);
        await tableData.refresh();
        setNotice({ tone: "success", text: hadEdits ? "Changes saved" : "Row inserted" });
      } catch (err) {
        setNotice({ tone: "error", text: typeof err === "string" ? err : "Couldn't insert the row" });
      }
      return;
    }
    if (hadEdits) setNotice({ tone: "success", text: "Changes saved" });
  }, [api, draft, draftCol, draftIsNull, draftText, editing, pending, schema, table, tableData]);

  const saveAllRef = useRef(saveAll);
  saveAllRef.current = saveAll;

  useEffect(() => {
    if (error) setNotice({ tone: "error", text: error });
  }, [error]);

  useEffect(() => {
    if (draftCol) draftInputRef.current?.focus();
  }, [draftCol]);

  useEffect(() => {
    if (!isActive) return;
    function onKeyDown(e: KeyboardEvent) {
      // The dialog editor handles its own keys
      if (editorDialogOpen || draftExpanded) return;
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        void saveAllRef.current();
      }
      if (e.key === "Escape" && editing.editingCell) editing.cancelEdit();
      if (e.key === "Escape" && draftCol) setDraftCol(null);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [editing, isActive, editorDialogOpen, draftExpanded, draftCol]);

  // Pasting rows (from a spreadsheet, say) onto the table offers to insert them
  useEffect(() => {
    if (!isActive || readOnly || importOpen !== false) return;
    function onPaste(e: ClipboardEvent) {
      if (editing.editingCell || (e.target as HTMLElement)?.closest("input, textarea, select, [contenteditable=true], [role=dialog]")) return;
      const text = e.clipboardData?.getData("text/plain") ?? "";
      // A single value isn't rows
      if (!text.includes("\t") && !text.trim().includes("\n")) return;
      e.preventDefault();
      setImportOpen({ text });
    }
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [isActive, readOnly, importOpen, editing.editingCell]);

  const fetchAllRows = useCallback(async () => {
    // An estimated total could miss rows, so count exactly before exporting everything
    const total = data?.totalRowsEstimated
      ? await api.countTableRows(schema!, table!)
      : (data?.totalRows ?? 0);
    const batchSize = 100;
    const pages = Math.ceil(total / batchSize);
    const params = {
      pageSize: batchSize,
      sort: toSortKeys(tableData.sorting),
      filters: tableData.filters,
      filterMode: tableData.filterMode,
    };

    const concurrency = 5;
    let columns: QueryColumn[] = [];
    const allRows: unknown[][] = new Array(total);
    let offset = 0;

    for (let i = 0; i < pages; i += concurrency) {
      const chunk = Array.from(
        { length: Math.min(concurrency, pages - i) },
        (_, j) => i + j + 1,
      );
      const results = await Promise.all(
        chunk.map((p) => api.getTableData(schema!, table!, { ...params, page: p })),
      );
      for (const res of results) {
        if (!columns.length) columns = res.columns;
        for (const row of res.rows) allRows[offset++] = row;
      }
    }

    return { columns, rows: allRows.slice(0, offset) };
  }, [schema, table, data?.totalRows, data?.totalRowsEstimated, tableData.sorting, tableData.filters, tableData.filterMode, api]);

  const columns = useMemo<ColumnDef<unknown[], unknown>[]>(() => {
    if (!data?.columns) return [];
    return layout.visible.map(({ column }) => ({
      id: column.name,
      header: column.name,
      accessorFn: (row: unknown[]) => row,
    }));
  }, [data?.columns, layout.visible]);

  const reactTable = useReactTable({
    data: data?.rows ?? [],
    columns,
    state: { sorting: tableData.sorting },
    onSortingChange: guard(tableData.setSorting),
    getCoreRowModel: getCoreRowModel(),
    manualSorting: true,
    manualPagination: true,
    // Shift+click adds a column to the sort instead of replacing it
    enableMultiSort: true,
    // Ascending first for every type (otherwise it's guessed from the first row)
    sortDescFirst: false,
    isMultiSortEvent: (e) => (e as MouseEvent).shiftKey,
  });

  /** The row as shown on screen, pending edits included. */
  const shownRow = (rowIndex: number) =>
    data!.columns.map((col, i) => {
      const pendingValue = pending.pendingChanges[rowIndex]?.[col.name];
      return pendingValue !== undefined ? pendingValue : data!.rows[rowIndex][i];
    });

  const menuItems = ((): ContextMenuItem[] => {
    if (!menu || !data) return [];
    const { rowIndex, colName } = menu;
    const colIdx = data.columns.findIndex((c) => c.name === colName);
    const column = columnInfos.find((c) => c.name === colName);
    const row = shownRow(rowIndex);
    const value = row[colIdx];
    // Right-clicking inside a multi-row selection acts on the whole selection
    const targetRows =
      selection.selectedRows.has(rowIndex) && selection.selectedRows.size > 1
        ? [...selection.selectedRows].sort((a, b) => a - b).map(shownRow)
        : [row];
    const rowsLabel = targetRows.length > 1 ? `${targetRows.length} rows` : "row";
    const copy = (text: string) => writeClipboardText(text);
    const link = foreignKeyLink(column, row, data.columns);
    const editable = !editLocked && editorFor(column, value).kind !== "readonly";

    const items: ContextMenuItem[] = [
      { label: "Copy value", icon: <Copy />, onSelect: () => copy(valueText(value)) },
      {
        label: `Copy ${rowsLabel} as`,
        icon: <Copy />,
        items: [
          { label: "JSON", icon: <Braces />, onSelect: () => copy(rowsToJson(data.columns, targetRows)) },
          { label: "TSV (for spreadsheets)", icon: <Sheet />, onSelect: () => copy(rowsToTsv(data.columns, targetRows)) },
          { label: "SQL INSERT", icon: <Database />, onSelect: () => copy(rowsToInsertSql(schema!, table!, data.columns, targetRows)) },
        ],
      },
      "separator",
      {
        label: value === null ? `Filter: ${colName} is NULL` : "Filter by this value",
        icon: <Filter />,
        onSelect: guard(() => {
          const filter: TableFilter =
            value === null
              ? { column: colName, operator: "is null", value: "" }
              : { column: colName, operator: "=", value: toEditText(value) };
          // Added to "match all" filters; with "match any" it would widen the result instead
          const keep = tableData.filterMode === "all" ? tableData.filters : [];
          tableData.applyFilters([...keep, filter], "all");
          setFiltersOpen(true);
        }),
      },
    ];
    if (link) {
      items.push({
        label: `Open referenced row in ${column!.foreignKey!.table}`,
        icon: <ArrowUpRight />,
        onSelect: guard(() => navigate(link.path, { state: { filters: link.filters } })),
      });
    }
    items.push("separator");
    if (editable && (column?.isNullable ?? true)) {
      items.push({
        label: "Set to NULL",
        icon: <Ban />,
        disabled: value === null,
        onSelect: () => editing.setCellValue(rowIndex, colName, true, ""),
      });
    }
    if (!readOnly) items.push({
      label: "Clone row",
      icon: <CopyPlus />,
      onSelect: guard(() => {
        const initialValues: Record<string, string | null> = {};
        data.columns.forEach((col, i) => {
          const info = columnInfos.find((c) => c.name === col.name);
          // Generated values and keys with a default (serial, uuid) must get new ones
          if (!info || info.isGenerated || (info.isPrimaryKey && info.defaultValue !== null)) return;
          initialValues[col.name] = row[i] === null || row[i] === undefined ? null : toEditText(row[i]);
        });
        openDraft(initialValues);
      }),
    });
    return items;
  })();

  return (
    <div className="h-full flex flex-col">
      <TableViewHeader
        schema={schema!}
        table={table!}
        relationType={relationType}
        hasNoPk={hasNoPk}
        readOnly={connectionReadOnly}
        selectedCount={selection.selectedRows.size}
        deleting={pending.deleting}
        onDelete={async () => {
          const count = selection.selectedRows.size;
          const err = await pending.handleDelete();
          setNotice(err ? { tone: "error", text: err } : { tone: "success", text: `Deleted ${count} row${count === 1 ? "" : "s"}` });
        }}
        hasPendingChanges={pending.hasPendingChanges || draft !== null}
        dirtyRowCount={pending.dirtyRowCount + (draft ? 1 : 0)}
        saving={pending.saving}
        onSave={() => void saveAll()}
        onDiscard={() => {
          pending.discardChanges();
          editing.cancelEdit();
          setDraft(null);
          setDraftCol(null);
          setDraftExpanded(false);
        }}
        onReviewSql={() => {
          setSqlCopied(false);
          setReviewSql(pendingChangesToSql(schema!, table!, pending.changedRows()));
        }}
        onInsertRow={guard(() => openDraft())}
        filterCount={tableData.filters.length}
        filtersOpen={filtersOpen}
        onToggleFilters={() => setFiltersOpen((open) => !open)}
      />

      {filtersOpen && columnInfos.length > 0 && (
        <FilterBar
          columns={columnInfos.map((c) => c.name)}
          applied={tableData.filters}
          mode={tableData.filterMode}
          onApply={guard(tableData.applyFilters)}
        />
      )}

      {error && !data && (
        <div className="px-4 py-3 text-sm text-muted-foreground">Couldn't load this table.</div>
      )}

      {loading && !data && <TableSkeleton columns={6} rows={14} />}

      {data && (
        <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
          <div ref={gridRef} className="flex-1 overflow-auto">
            <table
              className="w-full text-xs"
              style={{
                tableLayout: "fixed",
                borderCollapse: "separate",
                borderSpacing: 0,
                minWidth:
                  40 +
                  layout.visible.reduce(
                    (sum, { column }) => sum + resize.getColWidth(column.name),
                    0,
                  ),
              }}
            >
              <colgroup>
                <col style={{ width: 40 }} />
                {layout.visible.map(({ column }) => (
                  <col
                    key={column.name}
                    style={{ width: resize.getColWidth(column.name) }}
                  />
                ))}
              </colgroup>

              <thead className="sticky top-0 z-10 select-none">
                {reactTable.getHeaderGroups().map((hg) => (
                  <tr key={hg.id}>
                    <th
                      className="bg-card-header border-b border-r border-border px-3 py-2.5 text-left"
                      style={{ width: 40 }}
                    >
                      <Checkbox
                        checked={selection.allSelected}
                        indeterminate={selection.someSelected}
                        onChange={selection.toggleSelectAll}
                      />
                    </th>
                    {hg.headers.map((header) => (
                      <SortableColumnHeader
                        key={header.id}
                        id={header.id}
                        columnInfo={columnInfos.find((c) => c.name === header.id)}
                        sorted={header.column.getIsSorted()}
                        sortIndex={tableData.sorting.length > 1 ? header.column.getSortIndex() : -1}
                        selected={selectedColumn === header.id}
                        onSelect={() => setSelectedColumn((current) => (current === header.id ? null : header.id))}
                        onToggleSort={header.column.getToggleSortingHandler()!}
                        onResizeStart={(e) => resize.onResizeStart(e, header.id)}
                      />
                    ))}
                  </tr>
                ))}
              </thead>

              <tbody>
                {draft && (
                  <tr className="bg-success/15">
                    <td className="border-b border-r border-border px-3 py-1 text-success-foreground">
                      <Plus size={14} aria-label="New row" />
                    </td>
                    {layout.visible.map(({ column: col }) => {
                      const info = columnInfos.find((c) => c.name === col.name);
                      const current = col.name in draft ? draft[col.name] : null;
                      const editingThis = draftCol === col.name;
                      const editor = editorFor(info, current);
                      return (
                        <EditableCell
                          key={col.name}
                          value={current}
                          displayValue={current}
                          isDirty={col.name in draft}
                          isEditing={editingThis}
                          hasNoPk={editor.kind === "readonly"}
                          editor={editingThis && draftExpanded ? { kind: "long", json: isJsonType(info) } : editor}
                          nullable={info?.isNullable ?? true}
                          editValue={draftText}
                          editIsNull={draftIsNull}
                          inputRef={draftInputRef}
                          onStartEdit={() => {
                            if (editor.kind === "readonly") return;
                            setDraftCol(col.name);
                            setDraftText(current ?? "");
                            setDraftIsNull(current === null);
                            setDraftExpanded(editor.kind === "long");
                          }}
                          onEditValueChange={(value) => {
                            setDraftText(value);
                            setDraftIsNull(false);
                          }}
                          onSetNull={() => {
                            setDraftText("");
                            setDraftIsNull(true);
                          }}
                          onConfirm={() => commitDraft(col.name, draftIsNull, draftText)}
                          onCommit={(isNull, text) => commitDraft(col.name, isNull, text)}
                          onCancel={() => {
                            setDraftCol(null);
                            setDraftExpanded(false);
                          }}
                          onExpand={() => setDraftExpanded(true)}
                          columnName={col.name}
                          columnSelected={selectedColumn === col.name}
                          columnSelectionEnd={selectedColumn === col.name && data.rows.length === 0}
                          onContextMenu={() => {}}
                        />
                      );
                    })}
                  </tr>
                )}
                {data.rows.map((row, rowIndex) => {
                  const isRowDirty = !!pending.pendingChanges[rowIndex];
                  const isSelected = selection.selectedRows.has(rowIndex);
                  const rowBg = isSelected
                    ? "bg-primary/8"
                    : isRowDirty
                      ? "bg-warning/5"
                      : rowIndex % 2 === 0
                        ? "bg-card"
                        : "bg-muted";

                  return (
                    <tr
                      key={rowIndex}
                      className={`${rowBg} group/row hover:bg-accent`}
                    >
                      <td className="border-b border-r border-border px-3 py-1">
                        <Checkbox
                          checked={isSelected}
                          onChange={() => selection.toggleSelectRow(rowIndex)}
                        />
                      </td>
                      {layout.visible.map(({ column: col, index: colIdx }) => {
                        const originalValue = row[colIdx];
                        const columnInfo = columnInfos.find((c) => c.name === col.name);
                        const link = foreignKeyLink(columnInfo, row, data.columns);
                        const isDirty =
                          pending.pendingChanges[rowIndex]?.[col.name] !==
                          undefined;
                        const displayValue = isDirty
                          ? pending.pendingChanges[rowIndex][col.name]
                          : originalValue;
                        const isEditing =
                          editing.editingCell?.rowIndex === rowIndex &&
                          editing.editingCell?.colName === col.name;
                        const editor = isEditing && editingEditor ? editingEditor : editorFor(columnInfo, displayValue);

                        return (
                          <EditableCell
                            key={col.name}
                            value={originalValue}
                            displayValue={displayValue}
                            isDirty={isDirty}
                            isEditing={isEditing}
                            hasNoPk={editLocked}
                            editor={editor}
                            nullable={columnInfo?.isNullable ?? true}
                            editValue={editing.editValue}
                            editIsNull={editing.editIsNull}
                            inputRef={editing.inputRef}
                            onStartEdit={() =>
                              editing.startEdit(rowIndex, col.name, originalValue)
                            }
                            onEditValueChange={editing.setEditValue}
                            onSetNull={editing.setEditNull}
                            onConfirm={editing.confirmEdit}
                            onCommit={editing.commitEdit}
                            onCancel={editing.cancelEdit}
                            onExpand={() => setExpandedCell(editing.editingCell)}
                            linkLabel={link ? `Open the referenced row in ${columnInfo!.foreignKey!.table}` : undefined}
                            menuOpen={menu?.rowIndex === rowIndex && menu.colName === col.name}
                            columnName={col.name}
                            columnSelected={selectedColumn === col.name}
                            columnSelectionEnd={selectedColumn === col.name && rowIndex === data.rows.length - 1}
                            onContextMenu={(e) => setMenu({ x: e.clientX, y: e.clientY, rowIndex, colName: col.name })}
                            onFollowLink={
                              link ? guard(() => navigate(link.path, { state: { filters: link.filters } })) : undefined
                            }
                          />
                        );
                      })}
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {data.rows.length === 0 && !loading && (
              <div className="flex flex-col items-center gap-2 px-4 py-12 text-center">
                <p className="text-sm font-medium">
                  {tableData.filters.length > 0 ? "No rows match the filters" : "This table has no rows"}
                </p>
                {tableData.filters.length > 0 && (
                  <Button variant="outline" size="sm" onClick={guard(() => tableData.applyFilters([], "all"))}>
                    Clear filters
                  </Button>
                )}
              </div>
            )}
          </div>

          <Pagination
            page={tableData.page}
            totalPages={tableData.totalPages}
            estimated={data.totalRowsEstimated}
            hasNext={data.totalRowsEstimated ? data.rows.length === tableData.pageSize : undefined}
            onPageChange={guard(tableData.setPage)}
            pageSize={tableData.pageSize}
            pageSizeOptions={tableData.pageSizeOptions}
            onPageSizeChange={guard(tableData.setPageSize)}
            start={
              <TableTools
                schema={schema!}
                table={table!}
                totalRows={data.totalRows}
                totalRowsEstimated={data.totalRowsEstimated}
                counting={tableData.counting}
                onCountExactly={tableData.countExactly}
                refreshing={tableData.refreshing}
                loading={loading}
                spinKey={tableData.spinKey}
                onRefresh={guard(tableData.refresh)}
                columnsMenu={<ColumnsMenu layout={layout} />}
                readOnly={readOnly}
                onImportCsv={guard(() => setImportOpen({}))}
                hasCustomWidths={resize.hasCustomWidths}
                onResetWidths={resize.resetWidths}
                columns={data.columns}
                rows={data.rows}
                onFetchAllRows={fetchAllRows}
              />
            }
          />
        </div>
      )}

      <ContextMenu position={menu} items={menuItems} onClose={closeMenu} />

      <Dialog
        open={reviewSql !== null}
        onClose={() => setReviewSql(null)}
        className="max-w-2xl"
        title="Pending changes as SQL"
        description="What saving will run. Values are sent as parameters; this text is for reviewing or reusing."
        footer={
          <>
            <Button
              variant="outline"
              size="sm"
              className="mr-auto"
              onClick={async () => {
                await writeClipboardText(reviewSql ?? "");
                setSqlCopied(true);
              }}
            >
              {sqlCopied ? "Copied" : "Copy SQL"}
            </Button>
            <Button variant="neutral" size="sm" onClick={() => setReviewSql(null)}>
              Close
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => {
                setReviewSql(null);
                pending.handleSave();
              }}
            >
              Save changes
            </Button>
          </>
        }
      >
        <div className="h-64 overflow-hidden rounded-lg border border-border">
          <SqlViewer value={reviewSql ?? ""} />
        </div>
      </Dialog>

      <CellEditorDialog
        open={editorDialogOpen || draftExpanded}
        column={draftExpanded ? (draftCol ?? "") : (editingCell?.colName ?? "")}
        typeLabel={
          draftExpanded
            ? shortType(columnInfos.find((c) => c.name === draftCol)?.typeName ?? "")
            : editingColumn
              ? shortType(editingColumn.typeName)
              : undefined
        }
        // Starts from the inline draft, which begins as the cell's (pending) value
        value={draftExpanded ? (draftIsNull ? null : draftText) : editing.editIsNull ? null : editing.editValue}
        json={draftExpanded ? isJsonType(columnInfos.find((c) => c.name === draftCol)) : editingEditor?.kind === "long" && editingEditor.json}
        nullable={(draftExpanded ? columnInfos.find((c) => c.name === draftCol)?.isNullable : editingColumn?.isNullable) ?? true}
        onApply={(isNull, text) => (draftExpanded && draftCol ? commitDraft(draftCol, isNull, text) : editing.commitEdit(isNull, text))}
        onCancel={() => {
          if (draftExpanded) {
            setDraftExpanded(false);
            setDraftCol(null);
          } else editing.cancelEdit();
        }}
      />

      <ImportCsvDialog
        open={importOpen !== false}
        // Remounted for each paste, so it starts from the new data
        key={importOpen ? importOpen.text ?? "file" : "closed"}
        initialText={importOpen ? importOpen.text : undefined}
        schema={schema!}
        table={table!}
        columns={columnInfos}
        onClose={() => setImportOpen(false)}
        onImport={async (columns, rows) => {
          const count = await api.importRows(schema!, table!, columns, rows);
          setNotice({ tone: "success", text: `Imported ${count.toLocaleString()} row${count === 1 ? "" : "s"}` });
          await tableData.refresh();
          return count;
        }}
      />

      <NoticeToast notice={notice} onDismiss={dismissNotice} />

      <ConfirmDialog
        open={discardGuard.isConfirming}
        onClose={discardGuard.cancel}
        onConfirm={discardGuard.confirm}
        icon={<TriangleAlert />}
        tone="destructive"
        title="Discard unsaved changes?"
        description={`${pending.dirtyRowCount} row${pending.dirtyRowCount > 1 ? "s have" : " has"} unsaved edits. Loading other rows would discard them.`}
        cancelLabel="Keep editing"
        confirmLabel="Discard changes"
      />
    </div>
  );
}
