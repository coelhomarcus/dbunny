import { useState } from "react";
import { Plus, X } from "lucide-react";
import type { FilterMode, TableFilter } from "@/types";
import { emptyFilter, FILTER_OPERATORS, operatorFor } from "../../lib/tableFilters";
import Button from "../ui/Button";
import Input from "../ui/Input";
import SegmentedTabs from "../ui/SegmentedTabs";
import { selectClass } from "../ui/styles";

/** Filter conditions above the grid. Nothing is sent until Apply (or Enter). */
export default function FilterBar({
  columns,
  applied,
  mode,
  onApply,
}: {
  columns: string[];
  applied: TableFilter[];
  mode: FilterMode;
  onApply: (filters: TableFilter[], mode: FilterMode) => void;
}) {
  const [draft, setDraft] = useState<TableFilter[]>(() =>
    applied.length > 0 ? applied : [emptyFilter(columns[0] ?? "")],
  );
  const [draftMode, setDraftMode] = useState<FilterMode>(mode);

  function update(index: number, patch: Partial<TableFilter>) {
    setDraft((rows) => rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  function remove(index: number) {
    setDraft((rows) => (rows.length === 1 ? [emptyFilter(columns[0] ?? "")] : rows.filter((_, i) => i !== index)));
  }

  function clear() {
    setDraft([emptyFilter(columns[0] ?? "")]);
    setDraftMode("all");
    onApply([], "all");
  }

  return (
    <div className="shrink-0 space-y-2 border-b border-border bg-muted px-4 py-2.5">
      {draft.map((filter, index) => {
        const operator = operatorFor(filter.operator);
        return (
          <div key={index} className="flex items-center gap-2">
            <span className="w-12 shrink-0 text-right text-xs text-muted-foreground">
              {index === 0 ? "Where" : draftMode === "all" ? "and" : "or"}
            </span>
            <select
              aria-label="Column"
              value={filter.column}
              onChange={(e) => update(index, { column: e.target.value })}
              className={`${selectClass} w-44`}
            >
              {columns.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <select
              aria-label="Operator"
              value={filter.operator}
              onChange={(e) =>
                update(index, {
                  operator: e.target.value,
                  // A leftover value would be sent along with a null check
                  ...(operatorFor(e.target.value).noValue ? { value: "" } : {}),
                })
              }
              className={`${selectClass} w-44`}
            >
              {FILTER_OPERATORS.map((op) => (
                <option key={op.value} value={op.value}>
                  {op.label}
                </option>
              ))}
            </select>
            {!operator.noValue && (
              <Input
                aria-label="Value"
                value={filter.value}
                placeholder={operator.placeholder ?? "value"}
                spellCheck={false}
                autoFocus={index === draft.length - 1 && draft.length > 1}
                onChange={(e) => update(index, { value: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    onApply(draft, draftMode);
                  }
                }}
                className="max-w-80 flex-1 font-mono text-xs"
              />
            )}
            <Button variant="ghost-muted" size="icon-sm" aria-label="Remove filter" onClick={() => remove(index)}>
              <X />
            </Button>
          </div>
        );
      })}

      <div className="flex items-center gap-2 pl-14">
        <Button variant="ghost-muted" size="sm" onClick={() => setDraft((rows) => [...rows, emptyFilter(columns[0] ?? "")])}>
          <Plus />
          Add filter
        </Button>
        {draft.length > 1 && (
          <SegmentedTabs
            value={draftMode}
            onChange={setDraftMode}
            options={[
              { value: "all", label: "Match all" },
              { value: "any", label: "Match any" },
            ]}
          />
        )}
        <div className="flex-1" />
        <Button variant="ghost-muted" size="sm" onClick={clear}>
          Clear
        </Button>
        <Button variant="primary" size="sm" onClick={() => onApply(draft, draftMode)}>
          Apply
        </Button>
      </div>
    </div>
  );
}
