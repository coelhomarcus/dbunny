import { useEffect, type ReactNode, useEffectEvent } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { cn } from "../../lib/cn";
import Button from "./Button";
import { useT } from "../../i18n";

type Tone = "neutral" | "info" | "warning" | "destructive";

// Tinted tile behind the header icon
const TONES: Record<Tone, string> = {
  neutral: "bg-muted text-muted-foreground",
  info: "bg-info/12 text-info-foreground",
  warning: "bg-warning/14 text-warning-foreground",
  destructive: "bg-destructive/12 text-destructive-foreground",
};

/** Centred modal: click outside or press Escape to close. */
export default function Dialog({
  open,
  onClose,
  title,
  description,
  icon,
  tone = "neutral",
  children,
  footer,
  className,
  overlayClassName,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  /** Icon in a tinted tile before the title. */
  icon?: ReactNode;
  tone?: Tone;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
  /** Extra classes on the backdrop, for example to sit above another dialog. */
  overlayClassName?: string;
}) {
  const t = useT();
  // Escape closes with whatever onClose is now, without the listener being set again each render
  const closeOnEscape = useEffectEvent(onClose);
  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") closeOnEscape();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open]);

  if (!open) return null;

  return createPortal(
    <div className={cn("fixed inset-0 z-50 flex items-center justify-center p-4", overlayClassName)}>
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] animate-fade-in" onMouseDown={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        className={cn(
          "relative flex max-h-full w-full max-w-md flex-col overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl animate-dialog-in",
          className,
        )}
      >
        <div className="flex items-start gap-3 px-5 pt-5 pb-1">
          {icon && (
            <span className={cn("flex size-9 shrink-0 items-center justify-center rounded-full [&_svg]:size-[18px]", TONES[tone])}>
              {icon}
            </span>
          )}
          <div className={cn("min-w-0 flex-1", icon && "pt-0.5")}>
            <h2 className="text-[15px] font-semibold leading-tight">{title}</h2>
            {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
          </div>
          <Button variant="ghost-muted" size="icon-xs" className="-mr-1.5 -mt-1" aria-label={t("common.close")} onClick={onClose}>
            <X />
          </Button>
        </div>
        {children && <div className="min-h-0 overflow-y-auto px-5 py-3">{children}</div>}
        <div className={cn(!children && "pt-3")} />
        {footer && <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border bg-muted px-5 py-3">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}
