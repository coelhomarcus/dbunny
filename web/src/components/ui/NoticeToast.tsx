import { useEffect } from "react";
import { CircleAlert, CircleCheck, X } from "lucide-react";
import { cn } from "../../lib/cn";
import Button from "./Button";

export type Notice = { tone: "success" | "error"; text: string };

/** A result card in the bottom-right, above the status bar. Errors stay until dismissed. */
export default function NoticeToast({ notice, onDismiss }: { notice: Notice | null; onDismiss: () => void }) {
  useEffect(() => {
    if (notice?.tone !== "success") return;
    const timer = setTimeout(onDismiss, 4000);
    return () => clearTimeout(timer);
  }, [notice, onDismiss]);

  if (!notice) return null;

  const error = notice.tone === "error";
  return (
    <div
      role="status"
      className={cn(
        "fixed bottom-10 right-4 z-[60] flex w-96 max-w-[calc(100vw-2rem)] items-start gap-2.5 rounded-xl border bg-popover p-3.5 text-popover-foreground shadow-xl animate-dialog-in",
        error ? "border-destructive/40" : "border-border",
      )}
    >
      {error ? (
        <CircleAlert size={18} className="mt-0.5 shrink-0 text-destructive-foreground" />
      ) : (
        <CircleCheck size={18} className="mt-0.5 shrink-0 text-success-foreground" />
      )}
      <p className="min-w-0 flex-1 select-text break-words text-[13px] leading-snug">{notice.text}</p>
      <Button variant="ghost-muted" size="icon-xs" className="-mr-1 -mt-1" aria-label="Dismiss" onClick={onDismiss}>
        <X />
      </Button>
    </div>
  );
}
