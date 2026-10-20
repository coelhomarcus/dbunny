import type { ServerNotice } from "@/types";
import { cn } from "../../lib/cn";
import { useT } from "../../i18n";

const SEVERITY: Record<string, string> = {
  WARNING: "text-warning-foreground",
  NOTICE: "text-info-foreground",
  INFO: "text-info-foreground",
};

/** Messages the server sent while a query ran (RAISE NOTICE, warnings). */
export default function NoticeList({ notices }: { notices: ServerNotice[] }) {
  const t = useT();
  if (notices.length === 0) {
    return <p className="px-4 py-10 text-center text-sm text-muted-foreground">{t("query.noticeList.theServerSentNoMessages")}</p>;
  }
  return (
    <ol className="select-text divide-y divide-border font-mono text-xs">
      {notices.map((notice, i) => (
        <li key={i} className="flex gap-3 px-4 py-2">
          <span className={cn("w-16 shrink-0 font-semibold", SEVERITY[notice.severity] ?? "text-muted-foreground")}>
            {notice.severity}
          </span>
          <div className="min-w-0 flex-1 space-y-0.5">
            <p className="whitespace-pre-wrap break-words text-foreground">{notice.message}</p>
            {notice.detail && <p className="text-muted-foreground">{t("query.noticeList.detail", { text: notice.detail })}</p>}
            {notice.hint && <p className="text-muted-foreground">{t("query.noticeList.hint", { text: notice.hint })}</p>}
          </div>
        </li>
      ))}
    </ol>
  );
}
