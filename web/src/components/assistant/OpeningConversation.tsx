import { Loader2 } from "lucide-react";

function Bar({ className }: { className: string }) {
  return <div className={`h-3 animate-pulse rounded bg-muted ${className}`} />;
}

/** A past conversation on its way: its name and the shape of what is coming, from the click. */
export default function OpeningConversation({ title }: { title: string }) {
  return (
    <div className="min-h-0 flex-1 overflow-hidden px-3 py-3" aria-busy="true">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 size={13} className="shrink-0 animate-spin" />
        <span className="min-w-0 truncate">{title ? `Opening “${title}”…` : "Opening the conversation…"}</span>
      </div>
      <div className="mt-5 space-y-5">
        <div className="flex justify-end">
          <div className="w-3/5 space-y-2 rounded-2xl rounded-br-md bg-muted/60 px-3 py-3">
            <Bar className="w-full bg-muted-foreground/15" />
            <Bar className="w-2/3 bg-muted-foreground/15" />
          </div>
        </div>
        <div className="space-y-2">
          <Bar className="w-11/12" />
          <Bar className="w-full" />
          <Bar className="w-3/4" />
        </div>
        <div className="h-9 w-full animate-pulse rounded-lg border border-border" />
        <div className="space-y-2">
          <Bar className="w-5/6" />
          <Bar className="w-1/2" />
        </div>
      </div>
    </div>
  );
}
