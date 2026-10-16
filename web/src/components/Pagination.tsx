import Button from "./ui/Button";
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
          className="h-7 bg-card border border-input rounded-lg px-1.5 text-foreground shadow-xs/5 hover:bg-card-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 cursor-pointer"
        >
          {pageSizeOptions.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      </div>

      {totalPages > 1 && (
        <div className="flex items-center gap-3">
          <span>
            Page {page} of {estimated ? "~" : ""}{totalPages}
          </span>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => onPageChange(Math.max(1, page - 1))} disabled={page <= 1}>
              Previous
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onPageChange(estimated ? page + 1 : Math.min(totalPages, page + 1))}
              disabled={!(hasNext ?? page < totalPages)}
            >
              Next
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
