import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { cn } from "../../lib/cn";
import Button from "./Button";

/** Small centred modal: click outside or press Escape to close. */
export default function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  className,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
}) {
  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[16vh]">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] animate-fade-in" onMouseDown={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        className={cn(
          "relative w-full max-w-md overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl animate-dialog-in",
          className,
        )}
      >
        <div className="flex items-start gap-3 px-4 pt-4 pb-1">
          <div className="min-w-0 flex-1">
            <h2 className="text-sm font-semibold">{title}</h2>
            {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
          </div>
          <Button variant="ghost-muted" size="icon-xs" className="-mr-1 -mt-0.5" aria-label="Close" onClick={onClose}>
            <X />
          </Button>
        </div>
        {children && <div className="px-4 py-3">{children}</div>}
        {footer && <div className="flex items-center justify-end gap-2 border-t border-border bg-muted px-4 py-3">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}
