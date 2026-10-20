import { useState } from "react";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import Button from "./ui/Button";
import Tooltip from "./Tooltip";
import { inputClass, selectClass } from "./ui/styles";
import { cn } from "../lib/cn";
import { useT } from "../i18n";
import { Rich } from "../i18n/Rich";

/** Page number field: goes to the typed page on Enter or when it loses focus. */
function PageInput({ page, totalPages, estimated, onPageChange }: {
  page: number;
  totalPages: number;
  estimated?: boolean;
  onPageChange: (page: number) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState(String(page));
  const go = () => {
    const n = parseInt(draft, 10);
    // An estimated count can be short, so it doesn't cap the page
    const target = Number.isFinite(n) ? Math.max(1, estimated ? n : Math.min(totalPages, n)) : page;
    setDraft(String(target));
    if (target !== page) onPageChange(target);
  };
  return (
    <input
      value={draft}
      inputMode="numeric"
      aria-label={t("ui.pagination.page")}
      onChange={(e) => setDraft(e.target.value.replace(/\D/g, ""))}
      onBlur={go}
      onKeyDown={(e) => {
        if (e.key === "Enter") go();
        if (e.key === "Escape") setDraft(String(page));
      }}
      className={cn(inputClass, "h-7 w-14 px-1.5 text-center tabular-nums dark:bg-card")}
    />
  );
}
interface PaginationProps {
  page: number;
  totalPages: number;
  /** The page count comes from an estimated total. */
  estimated?: boolean;
  /** Overrides "is there a next page", for when the total is only an estimate. */
  hasNext?: boolean;
  onPageChange: (page: number) => void;
  pageSize: number;
  pageSizeOptions: number[];
  onPageSizeChange: (size: number) => void;
  /** Shown on the left: row count and the table's secondary tools. */
  start?: React.ReactNode;
}

export default function Pagination({
  page,
  totalPages,
  estimated,
  hasNext,
  onPageChange,
  pageSize,
  pageSizeOptions,
  onPageSizeChange,
  start,
}: PaginationProps) {
  const t = useT();
  return (
    <div className="flex h-10 shrink-0 items-center gap-3 border-t border-border px-2.5 text-xs text-muted-foreground select-none">
      <div className="flex min-w-0 items-center gap-1">{start}</div>
      <div className="flex-1" />
      <label className="flex items-center gap-1.5">
        {t("ui.pagination.perPage")}
        <select
          value={pageSize}
          onChange={(e) => onPageSizeChange(Number(e.target.value))}
          className={cn(selectClass, "h-7 px-1.5 text-xs shadow-xs shadow-black/10")}
        >
          {pageSizeOptions.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      </label>

      {totalPages > 1 && (
        <div className="flex items-center gap-1.5">
          <Tooltip label={t("ui.pagination.firstPage")} side="top">
            <Button variant="outline" size="icon-sm" aria-label={t("ui.pagination.firstPage")} onClick={() => onPageChange(1)} disabled={page <= 1}>
              <ChevronsLeft />
            </Button>
          </Tooltip>
          <Tooltip label={t("ui.pagination.previousPage")} side="top">
            <Button variant="outline" size="icon-sm" aria-label={t("ui.pagination.previousPage")} onClick={() => onPageChange(Math.max(1, page - 1))} disabled={page <= 1}>
              <ChevronLeft />
            </Button>
          </Tooltip>
          <span className="flex items-center gap-1.5 px-1.5">
            <Rich
              id={estimated ? "ui.pagination.pageOfEstimated" : "ui.pagination.pageOf"}
              params={{ count: totalPages }}
              // Remounted when the page changes elsewhere, so the field shows it
              tags={{ field: () => <PageInput key={page} page={page} totalPages={totalPages} estimated={estimated} onPageChange={onPageChange} /> }}
            />
          </span>
          <Tooltip label={t("ui.pagination.nextPage")} side="top">
            <Button
              variant="outline"
              size="icon-sm"
              aria-label={t("ui.pagination.nextPage")}
              onClick={() => onPageChange(estimated ? page + 1 : Math.min(totalPages, page + 1))}
              disabled={!(hasNext ?? page < totalPages)}
            >
              <ChevronRight />
            </Button>
          </Tooltip>
          <Tooltip label={t("ui.pagination.lastPage")} side="top">
            <Button
              variant="outline"
              size="icon-sm"
              aria-label={t("ui.pagination.lastPage")}
              onClick={() => onPageChange(totalPages)}
              // The last page of an estimated count isn't known
              disabled={estimated || page >= totalPages}
            >
              <ChevronsRight />
            </Button>
          </Tooltip>
        </div>
      )}
    </div>
  );
}
