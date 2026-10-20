import { useEffect, useMemo, useState } from "react";
import { Plus, X } from "lucide-react";
import type { FilterMode, TableFilter } from "@/types";
import { cn } from "../../lib/cn";
import { emptyFilter, FILTER_OPERATORS, operatorFor } from "../../lib/tableFilters";
import Button from "../ui/Button";
import Input from "../ui/Input";
import SegmentedTabs from "../ui/SegmentedTabs";
import { selectClass } from "../ui/styles";
import { useT } from "../../i18n";

const LABEL_W = "w-[3.25rem]";

/** Compact field inside a grouped filter row (no double border). */
const fieldSelectClass = cn(
  selectClass,
  "h-7 min-w-0 shrink border-0 bg-transparent px-1.5 shadow-none hover:bg-card/80 focus-visible:ring-0",
);

const fieldInputClass =
  "h-7 min-w-0 flex-1 border-0 bg-transparent px-1.5 shadow-none font-mono text-xs hover:bg-card/80 focus-visible:ring-0";

function filtersEqual(a: TableFilter[], b: TableFilter[], modeA: FilterMode, modeB: FilterMode) {
  if (modeA !== modeB || a.length !== b.length) return false;
  return a.every((row, i) => {
    const other = b[i];
    return row.column === other.column && row.operator === other.operator && row.value === other.value;
  });
}

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
  const t = useT();
  const [draft, setDraft] = useState<TableFilter[]>(() =>
    applied.length > 0 ? applied : [emptyFilter(columns[0] ?? "")],
  );
  const [draftMode, setDraftMode] = useState<FilterMode>(mode);

  useEffect(() => {
    setDraft(applied.length > 0 ? applied : [emptyFilter(columns[0] ?? "")]);
    setDraftMode(mode);
  }, [applied, mode, columns]);

  const dirty = useMemo(() => !filtersEqual(draft, applied, draftMode, mode), [draft, applied, draftMode, mode]);

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

  function applyNow() {
    onApply(draft, draftMode);
  }

  return (
    <div className="shrink-0 border-b border-border bg-card px-4 py-3">
      <div className="mb-2.5 flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-[13px] font-medium text-foreground">{t("table.filterBar.filters")}</span>
          {dirty && (
            <span className="rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] font-medium text-primary">{t("table.filterBar.unapplied")}</span>
          )}
        </div>
        {draft.length > 1 && (
          <SegmentedTabs
            value={draftMode}
            onChange={setDraftMode}
            options={[
              { value: "all", label: t("table.filterBar.matchAll") },
              { value: "any", label: t("table.filterBar.matchAny") },
            ]}
          />
        )}
        <div className="flex flex-1 justify-end gap-1.5">
          <Button variant="ghost-muted" size="sm" onClick={clear}>
            {t("table.filterBar.clear")}
          </Button>
          <Button variant="primary" size="sm" onClick={applyNow} disabled={!dirty}>
            {t("table.filterBar.apply")}
          </Button>
        </div>
      </div>

      <div className="space-y-1.5">
        {draft.map((filter, index) => {
          const operator = operatorFor(filter.operator);
          const logic =
            index === 0 ? (
              <span className={cn(LABEL_W, "shrink-0 text-right text-[11px] font-medium text-muted-foreground")}>{t("table.filterBar.where")}</span>
            ) : (
              <span
                className={cn(
                  LABEL_W,
                  "shrink-0 text-right text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                )}
              >
                {draftMode === "all" ? "And" : "Or"}
              </span>
            );

          return (
            <div key={index} className="flex items-center gap-2">
              {logic}
              <div
                className={cn(
                  "flex min-w-0 flex-1 items-center gap-0.5 rounded-lg border border-input bg-muted/35 p-0.5",
                  "shadow-xs/5 dark:bg-muted/25",
                )}
              >
                <select
                  aria-label={t("table.filterBar.column")}
                  value={filter.column}
                  onChange={(e) => update(index, { column: e.target.value })}
                  className={cn(fieldSelectClass, "max-w-[11rem] flex-1 basis-32")}
                >
                  {columns.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
                <span className="h-5 w-px shrink-0 bg-border/80" aria-hidden />
                <select
                  aria-label={t("table.filterBar.operator")}
                  value={filter.operator}
                  onChange={(e) =>
                    update(index, {
                      operator: e.target.value,
                      ...(operatorFor(e.target.value).noValue ? { value: "" } : {}),
                    })
                  }
                  className={cn(fieldSelectClass, "max-w-[10rem] flex-1 basis-28")}
                >
                  {FILTER_OPERATORS.map((op) => (
                    <option key={op.value} value={op.value}>
                      {t(op.labelKey)}
                    </option>
                  ))}
                </select>
                {!operator.noValue && (
                  <>
                    <span className="h-5 w-px shrink-0 bg-border/80" aria-hidden />
                    <Input
                      aria-label={t("table.filterBar.value")}
                      value={filter.value}
                      placeholder={operator.placeholderKey ? t(operator.placeholderKey) : operator.placeholder ?? t("table.filterBar.value")}
                      spellCheck={false}
                      autoFocus={index === draft.length - 1 && draft.length > 1}
                      onChange={(e) => update(index, { value: e.target.value })}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          applyNow();
                        }
                      }}
                      className={fieldInputClass}
                    />
                  </>
                )}
                <Button
                  variant="ghost-muted"
                  size="icon-xs"
                  aria-label={t("table.filterBar.removeFilter")}
                  className="ml-0.5 shrink-0 text-muted-foreground"
                  onClick={() => remove(index)}
                >
                  <X />
                </Button>
              </div>
            </div>
          );
        })}
      </div>

      <div className="mt-2 flex items-center gap-2">
        <span className={cn(LABEL_W, "shrink-0")} />
        <Button variant="ghost-muted" size="sm" onClick={() => setDraft((rows) => [...rows, emptyFilter(columns[0] ?? "")])}>
          <Plus />
          {t("table.filterBar.addCondition")}
        </Button>
      </div>
    </div>
  );
}
