import { useEffect, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";

const GAP = 8;
const MARGIN = 8;

/**
 * Floating panel anchored to an element. Opens below it (above when there's no
 * room) and closes on Escape or a click outside both the panel and the anchor.
 */
export default function Popover({
  open,
  onClose,
  anchorRef,
  children,
  className,
}: {
  open: boolean;
  onClose: () => void;
  anchorRef: RefObject<HTMLElement | null>;
  children: ReactNode;
  className?: string;
}) {
  const panelRef = useRef<HTMLDivElement>(null);

  // Measure after render (the panel's own size is needed), then place it directly:
  // it's pure DOM positioning, no need for a re-render
  useLayoutEffect(() => {
    const panelEl = panelRef.current;
    const anchor = anchorRef.current?.getBoundingClientRect();
    if (!open || !panelEl || !anchor) return;
    const panel = panelEl.getBoundingClientRect();

    const below = anchor.bottom + GAP;
    const top = below + panel.height > window.innerHeight - MARGIN ? anchor.top - GAP - panel.height : below;
    const left = Math.min(Math.max(MARGIN, anchor.left), window.innerWidth - panel.width - MARGIN);
    panelEl.style.top = `${Math.max(MARGIN, top)}px`;
    panelEl.style.left = `${left}px`;
    panelEl.style.visibility = "visible";
  }, [open, anchorRef]);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      const target = e.target as Node;
      if (panelRef.current?.contains(target) || anchorRef.current?.contains(target)) return;
      onClose();
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose, anchorRef]);

  if (!open) return null;

  return createPortal(
    <div
      ref={panelRef}
      role="dialog"
      className={cn(
        "fixed z-50 rounded-xl border border-border bg-popover text-popover-foreground shadow-xl animate-dialog-in",
        className,
      )}
      // Hidden until positioned by the layout effect, so it never flashes at the corner
      style={{ top: 0, left: 0, visibility: "hidden" }}
    >
      {children}
    </div>,
    document.body,
  );
}
