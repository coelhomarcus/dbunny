import { useEffect, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";

const GAP = 8;
const DOCK_GAP = 4;
const MARGIN = 8;

function dockParent(anchor: HTMLElement): HTMLElement | null {
  const parent = anchor.offsetParent;
  return parent instanceof HTMLElement ? parent : null;
}

/**
 * Floating panel anchored to an element. Opens below it (above when there's no
 * room) and closes on Escape or a click outside both the panel and the anchor.
 *
 * With `dock`, the panel is portaled into the anchor's offset parent and sits
 * flush under the anchor at the same width (for sidebar dropdowns).
 */
export default function Popover({
  open,
  onClose,
  anchorRef,
  children,
  className,
  dock = false,
}: {
  open: boolean;
  onClose: () => void;
  anchorRef: RefObject<HTMLElement | null>;
  children: ReactNode;
  className?: string;
  /** Pin below the anchor inside its offset parent; match the anchor width. */
  dock?: boolean;
}) {
  const panelRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const panelEl = panelRef.current;
    const anchorEl = anchorRef.current;
    if (!open || !panelEl || !anchorEl) return;

    function place() {
      const panel = panelEl!;
      const anchor = anchorRef.current;
      if (!anchor) return;

      if (dock) {
        const parent = dockParent(anchor);
        if (parent) {
          const gap = DOCK_GAP;
          const top = anchor.offsetTop + anchor.offsetHeight + gap;
          const left = anchor.offsetLeft;
          const width = anchor.offsetWidth;
          const panelHeight = panel.offsetHeight;
          const roomBelow = parent.clientHeight - top;
          const flipped = panelHeight > 0 && roomBelow < panelHeight && anchor.offsetTop > panelHeight + gap;
          panel.style.position = "absolute";
          panel.style.left = `${left}px`;
          panel.style.width = `${width}px`;
          panel.style.top = flipped ? `${anchor.offsetTop - gap - panelHeight}px` : `${top}px`;
          panel.style.visibility = "visible";
          return;
        }
      }

      const rect = anchor.getBoundingClientRect();
      const box = panel.getBoundingClientRect();
      const below = rect.bottom + GAP;
      const top =
        below + box.height > window.innerHeight - MARGIN ? rect.top - GAP - box.height : below;
      const left = Math.min(Math.max(MARGIN, rect.left), window.innerWidth - box.width - MARGIN);
      panel.style.position = "fixed";
      panel.style.width = "";
      panel.style.top = `${Math.max(MARGIN, top)}px`;
      panel.style.left = `${left}px`;
      panel.style.visibility = "visible";
    }

    place();
    const observer = new ResizeObserver(place);
    observer.observe(panelEl);
    observer.observe(anchorEl);
    window.addEventListener("resize", place);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", place);
    };
  }, [open, anchorRef, dock, children]);

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

  const anchorEl = anchorRef.current;
  const portalTarget =
    dock && anchorEl ? dockParent(anchorEl) : null;

  return createPortal(
    <div
      ref={panelRef}
      role="dialog"
      className={cn(
        "z-50 rounded-xl border border-border bg-popover text-popover-foreground shadow-xl animate-dialog-in",
        dock ? "absolute" : "fixed",
        className,
      )}
      style={{ top: 0, left: 0, visibility: "hidden" }}
    >
      {children}
    </div>,
    portalTarget ?? document.body,
  );
}
