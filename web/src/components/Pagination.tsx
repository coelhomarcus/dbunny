interface PaginationProps {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  pageSize: number;
  pageSizeOptions: number[];
  onPageSizeChange: (size: number) => void;
}

export default function Pagination({
  page,
  totalPages,
  onPageChange,
  pageSize,
  pageSizeOptions,
  onPageSizeChange,
}: PaginationProps) {
  return (
    <div className="flex items-center justify-between px-4 py-2.5 border-t border-zinc-800/40 bg-zinc-800/30 text-sm text-zinc-400">
      <div className="flex items-center gap-2">
        <span>Rows per page</span>
        <select
          value={pageSize}
          onChange={(e) => onPageSizeChange(Number(e.target.value))}
          className="bg-zinc-700/50 border border-zinc-700/60 rounded-lg px-2 py-1 text-zinc-300 hover:bg-zinc-700 focus:outline-none cursor-pointer"
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
            Page {page} of {totalPages}
          </span>
          <div className="flex gap-2">
            <button
              onClick={() => onPageChange(Math.max(1, page - 1))}
              disabled={page <= 1}
              className="px-3 py-1.5 rounded-lg bg-zinc-700/50 hover:bg-zinc-700 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              Previous
            </button>
            <button
              onClick={() => onPageChange(Math.min(totalPages, page + 1))}
              disabled={page >= totalPages}
              className="px-3 py-1.5 rounded-lg bg-zinc-700/50 hover:bg-zinc-700 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
