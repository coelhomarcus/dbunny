import type { ServerNotice } from "@/types";
import { cn } from "../../lib/cn";

const SEVERITY: Record<string, string> = {
  WARNING: "text-warning-foreground",
  NOTICE: "text-info-foreground",
  INFO: "text-info-foreground",
};

/** Messages the server sent while a query ran (RAISE NOTICE, warnings). */
export default function NoticeList({ notices }: { notices: ServerNotice[] }) {
  if (notices.length === 0) {
    return <p className="px-4 py-10 text-center text-sm text-muted-foreground">The server sent no messages.</p>;
  }
  return (
    <ol className="divide-y divide-border font-mono text-xs">
      {notices.map((notice, i) => (
        <li key={i} className="flex gap-3 px-4 py-2">
          <span className={cn("w-16 shrink-0 font-semibold", SEVERITY[notice.severity] ?? "text-muted-foreground")}>
            {notice.severity}
          </span>
          <div className="min-w-0 flex-1 space-y-0.5">
            <p className="whitespace-pre-wrap break-words text-foreground">{notice.message}</p>
            {notice.detail && <p className="text-muted-foreground">Detail: {notice.detail}</p>}
            {notice.hint && <p className="text-muted-foreground">Hint: {notice.hint}</p>}
          </div>
        </li>
      ))}
    </ol>
  );
}
