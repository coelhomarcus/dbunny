import { useEffect, useMemo } from "react";
import { useParams } from "react-router";
import {
  useReactTable,
  getCoreRowModel,
  type ColumnDef,
} from "@tanstack/react-table";
import type { QueryColumn } from "@/types";
import { useTableData } from "../hooks/useTableData";
import { useCellEditing } from "../hooks/useCellEditing";
import { useRowSelection } from "../hooks/useRowSelection";
import { useColumnResize } from "../hooks/useColumnResize";
import { usePendingChanges } from "../hooks/usePendingChanges";
import Checkbox from "../components/Checkbox";
import TableViewHeader from "../components/TableViewHeader";
import SortableColumnHeader from "../components/SortableColumnHeader";
import EditableCell from "../components/EditableCell";
import Pagination from "../components/Pagination";
import { Loader } from "lucide-react";

export default function TableView() {
  const { schema, table } = useParams<{ schema: string; table: string }>();

  const tableData = useTableData(schema, table);
  const { data, columnInfos, loading, error, hasNoPk } = tableData;

  const selection = useRowSelection(data?.rows.length ?? 0);
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
  });

  const editing = useCellEditing({
    data,
    hasNoPk,
    pendingChanges: pending.pendingChanges,
    setPendingChanges: pending.setPendingChanges,
  });

  useEffect(() => {
    pending.setPendingChanges({});
    editing.cancelEdit();
    selection.setSelectedRows(new Set());
    resize.resetWidths();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, table]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (editing.editingCell) editing.confirmEdit();
        else pending.handleSave();
      }
      if (e.key === "Escape" && editing.editingCell) editing.cancelEdit();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [editing, pending]);

  const columns = useMemo<ColumnDef<unknown[], unknown>[]>(() => {
    if (!data?.columns) return [];
    return data.columns.map((col: QueryColumn) => ({
      id: col.name,
      header: col.name,
      accessorFn: (row: unknown[]) => row,
    }));
  }, [data?.columns]);

  const reactTable = useReactTable({
    data: data?.rows ?? [],
    columns,
    state: { sorting: tableData.sorting },
    onSortingChange: tableData.setSorting,
    getCoreRowModel: getCoreRowModel(),
    manualSorting: true,
    manualPagination: true,
  });

  return (
    <div className="h-full flex flex-col gap-2">
      <TableViewHeader
        schema={schema!}
        table={table!}
        totalRows={data?.totalRows}
        hasNoPk={hasNoPk}
        selectedCount={selection.selectedRows.size}
        deleting={pending.deleting}
        onDelete={pending.handleDelete}
        hasPendingChanges={pending.hasPendingChanges}
        dirtyRowCount={pending.dirtyRowCount}
        saveError={pending.saveError}
        saving={pending.saving}
        onSave={pending.handleSave}
        onDiscard={() => {
          pending.discardChanges();
          editing.cancelEdit();
        }}
        refreshing={tableData.refreshing}
        loading={loading}
        spinKey={tableData.spinKey}
        onRefresh={tableData.refresh}
        hasCustomWidths={resize.hasCustomWidths}
        onResetWidths={resize.resetWidths}
      />

      {error && <div className="p-4 text-red-400 text-sm bg-zinc-900 border border-zinc-800/60 rounded-xl">{error}</div>}

      {loading && !data && (
        <div className="flex-1 flex items-end justify-end p-4">
          <Loader size={16} className="animate-spin text-zinc-500" />
        </div>
      )}

      {data && (
        <div className="flex-1 min-h-0 flex flex-col bg-zinc-900 border border-zinc-800/60 rounded-xl overflow-hidden">
          <div className="flex-1 overflow-auto">
            <table
              className="text-sm w-full"
              style={{
                tableLayout: "fixed",
                borderCollapse: "separate",
                borderSpacing: 0,
                minWidth:
                  40 +
                  data.columns.reduce(
                    (sum, col) => sum + resize.getColWidth(col.name),
                    0,
                  ),
              }}
            >
              <colgroup>
                <col style={{ width: 40 }} />
                {data.columns.map((col) => (
                  <col
                    key={col.name}
                    style={{ width: resize.getColWidth(col.name) }}
                  />
                ))}
              </colgroup>

              <thead className="sticky top-0 z-10">
                {reactTable.getHeaderGroups().map((hg) => (
                  <tr key={hg.id}>
                    <th
                      className="bg-zinc-800 border-b border-r border-zinc-700/40 px-3 py-2.5 text-left"
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
                        onToggleSort={header.column.getToggleSortingHandler()!}
                        onResizeStart={(e) => resize.onResizeStart(e, header.id)}
                      />
                    ))}
                  </tr>
                ))}
              </thead>

              <tbody>
                {data.rows.map((row, rowIndex) => {
                  const isRowDirty = !!pending.pendingChanges[rowIndex];
                  const isSelected = selection.selectedRows.has(rowIndex);
                  const rowBg = isSelected
                    ? "bg-zinc-800/40"
                    : isRowDirty
                      ? "bg-amber-950/10"
                      : rowIndex % 2 === 0
                        ? "bg-zinc-900"
                        : "bg-zinc-800/20";

                  return (
                    <tr
                      key={rowIndex}
                      className={`${rowBg} hover:bg-zinc-800/40`}
                    >
                      <td className="border-b border-r border-zinc-800/40 px-3 py-1.5">
                        <Checkbox
                          checked={isSelected}
                          onChange={() => selection.toggleSelectRow(rowIndex)}
                        />
                      </td>
                      {data.columns.map((col, colIdx) => {
                        const originalValue = row[colIdx];
                        const isDirty =
                          pending.pendingChanges[rowIndex]?.[col.name] !==
                          undefined;
                        const displayValue = isDirty
                          ? pending.pendingChanges[rowIndex][col.name]
                          : originalValue;
                        const isEditing =
                          editing.editingCell?.rowIndex === rowIndex &&
                          editing.editingCell?.colName === col.name;

                        return (
                          <EditableCell
                            key={col.name}
                            value={originalValue}
                            displayValue={displayValue}
                            isDirty={isDirty}
                            isEditing={isEditing}
                            hasNoPk={hasNoPk}
                            editValue={editing.editValue}
                            inputRef={editing.inputRef}
                            onStartEdit={() =>
                              editing.startEdit(rowIndex, col.name, originalValue)
                            }
                            onEditValueChange={editing.setEditValue}
                            onConfirm={editing.confirmEdit}
                            onCancel={editing.cancelEdit}
                          />
                        );
                      })}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          <Pagination
            page={tableData.page}
            totalPages={tableData.totalPages}
            onPageChange={tableData.setPage}
          />
        </div>
      )}
    </div>
  );
}
