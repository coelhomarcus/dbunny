import { language } from "@twinkleplop/sql";
import { cn } from "../../lib/cn";

const highlight = language();

/** Read-only SQL, colored with the same tokens as the query editor. */
export default function SqlPreview({ source }: { source: string }) {
  return <div className="sql-preview" dangerouslySetInnerHTML={{ __html: highlight(source) }} />;
}

const boxClass = "mt-1 max-h-40 overflow-auto rounded-lg border border-border bg-muted px-3 py-2 font-mono text-xs";

/** The labeled SQL block under a schema form. An error replaces the highlight. */
export function SqlPreviewBox({ source, error }: { source: string; error: string | null }) {
  return (
    <div>
      <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">SQL</span>
      {source && !error ? (
        <div className={boxClass}>
          <SqlPreview source={source} />
        </div>
      ) : (
        <pre className={cn(boxClass, "whitespace-pre-wrap break-words text-muted-foreground")}>{error || "…"}</pre>
      )}
    </div>
  );
}
