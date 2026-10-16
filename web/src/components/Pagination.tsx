import { useState } from "react";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import Button from "./ui/Button";
import { inputClass, selectClass } from "./ui/styles";
import { cn } from "../lib/cn";

/** Page number field: goes to the typed page on Enter or when it loses focus. */
function PageInput({ page, totalPages, estimated, onPageChange }: {
  page: number;
  totalPages: number;
  estimated?: boolean;
  onPageChange: (page: number) => void;
}) {
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
      aria-label="Page"
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
}: PaginationProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-t border-border bg-muted text-sm text-muted-foreground select-none">
      <div className="flex items-center gap-2">
        <span>Rows per page</span>
        <select
          value={pageSize}
          onChange={(e) => onPageSizeChange(Number(e.target.value))}
          className={cn(selectClass, "h-7 px-1.5 shadow-xs shadow-black/10")}
        >
          {pageSizeOptions.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      </div>

      {totalPages > 1 && (
        <div className="flex items-center gap-1.5">
          <Button variant="outline" size="icon-sm" aria-label="First page" onClick={() => onPageChange(1)} disabled={page <= 1}>
            <ChevronsLeft />
          </Button>
          <Button variant="outline" size="icon-sm" aria-label="Previous page" onClick={() => onPageChange(Math.max(1, page - 1))} disabled={page <= 1}>
            <ChevronLeft />
          </Button>
          <span className="flex items-center gap-1.5 px-1.5">
            Page
            {/* Remounted when the page changes elsewhere, so the field shows it */}
            <PageInput key={page} page={page} totalPages={totalPages} estimated={estimated} onPageChange={onPageChange} />
            of {estimated ? "~" : ""}
            {totalPages.toLocaleString()}
          </span>
          <Button
            variant="outline"
            size="icon-sm"
            aria-label="Next page"
            onClick={() => onPageChange(estimated ? page + 1 : Math.min(totalPages, page + 1))}
            disabled={!(hasNext ?? page < totalPages)}
          >
            <ChevronRight />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            aria-label="Last page"
            onClick={() => onPageChange(totalPages)}
            // The last page of an estimated count isn't known
            disabled={estimated || page >= totalPages}
          >
            <ChevronsRight />
          </Button>
        </div>
      )}
    </div>
  );
}
