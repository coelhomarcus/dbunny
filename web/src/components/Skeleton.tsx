function SkeletonBar({ className = "" }: { className?: string }) {
  return (
    <div
      className={`h-3 rounded bg-accent animate-pulse ${className}`}
    />
  );
}

export function TableSkeleton({ columns = 5, rows = 12 }: { columns?: number; rows?: number }) {
  return (
    <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-auto">
        <table className="w-full text-sm" style={{ tableLayout: "fixed" }}>
          <thead className="sticky top-0 z-10">
            <tr>
              {Array.from({ length: columns }).map((_, i) => (
                <th
                  key={i}
                  className="text-left px-4 py-2.5 bg-card-header border-b border-border"
                >
                  <SkeletonBar className="w-20" />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: rows }).map((_, rowIdx) => (
              <tr
                key={rowIdx}
                className={`border-b border-border ${
                  rowIdx % 2 === 0 ? "bg-card" : "bg-muted"
                }`}
              >
                {Array.from({ length: columns }).map((_, colIdx) => (
                  <td key={colIdx} className="px-4 py-2.5">
                    <SkeletonBar
                      className={
                        colIdx % 3 === 0 ? "w-16" : colIdx % 3 === 1 ? "w-24" : "w-20"
                      }
                    />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export function DetailSkeleton() {
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="px-4 py-4 border-b border-border">
        <div className="grid grid-cols-2 gap-x-8 gap-y-4 max-w-xl">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex items-center gap-3">
              <SkeletonBar className="w-14" />
              <SkeletonBar className="w-24" />
            </div>
          ))}
        </div>
      </div>
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        <div className="flex h-9 items-center px-4 bg-muted border-b border-border">
          <SkeletonBar className="w-20" />
        </div>
        <div className="p-5 space-y-3">
          {Array.from({ length: 8 }).map((_, i) => (
            <SkeletonBar
              key={i}
              className={i % 2 === 0 ? "w-3/4" : "w-1/2"}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

export function QueryResultsSkeleton() {
  return (
    <div className="flex-1 min-h-0 overflow-auto flex flex-col">
      <table className="w-full text-sm">
        <thead className="sticky top-0 z-10">
          <tr>
            {Array.from({ length: 4 }).map((_, i) => (
              <th
                key={i}
                className="text-left px-3 py-2.5 bg-card-header border-b border-border"
              >
                <SkeletonBar className="w-20" />
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {Array.from({ length: 8 }).map((_, rowIdx) => (
            <tr
              key={rowIdx}
              className={`border-b border-border ${
                rowIdx % 2 === 0 ? "bg-card" : "bg-muted"
              }`}
            >
              {Array.from({ length: 4 }).map((_, colIdx) => (
                <td key={colIdx} className="px-3 py-2">
                  <SkeletonBar
                    className={colIdx % 2 === 0 ? "w-24" : "w-16"}
                  />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
