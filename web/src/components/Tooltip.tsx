import { useState, useRef, useLayoutEffect } from "react";
import { createPortal } from "react-dom";
import { cn } from "../lib/cn";

interface TooltipProps {
  label: string;
  children: React.ReactNode;
  side?: "right" | "left" | "top" | "bottom";
  className?: string;
}

/** Space between the element and its tooltip. */
const GAP = 4;
/** Closest a tooltip gets to the window's edges. */
const MARGIN = 8;

export default function Tooltip({ label, children, side = "right", className }: TooltipProps) {
  // The element's box while the tooltip shows
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);

  function show() {
    const el = ref.current;
    // A menu opened by the wrapped button covers the same spot, so it wins
    if (!el || el.querySelector("[aria-expanded='true']")) return;
    setAnchor(el.getBoundingClientRect());
  }

  // Placed once its own size is known: beside the element on its side, kept inside the window
  useLayoutEffect(() => {
    const tip = tipRef.current;
    if (!anchor || !tip) return;
    const { width, height } = tip.getBoundingClientRect();
    const r = anchor;
    let left: number;
    let top: number;
    if (side === "top" || side === "bottom") {
      left = r.left + r.width / 2 - width / 2;
      top = side === "top" ? r.top - GAP - height : r.bottom + GAP;
    } else {
      left = side === "right" ? r.right + GAP : r.left - GAP - width;
      top = r.top + r.height / 2 - height / 2;
    }
    left = Math.min(Math.max(MARGIN, left), window.innerWidth - width - MARGIN);
    top = Math.min(Math.max(MARGIN, top), window.innerHeight - height - MARGIN);
    tip.style.left = `${left}px`;
    tip.style.top = `${top}px`;
    tip.style.visibility = "visible";
  }, [anchor, side, label]);

  return (
    <div
      ref={ref}
      onMouseEnter={show}
      onMouseLeave={() => setAnchor(null)}
      // Clicking acts on the button (often opening a menu): the hint has done its job
      onMouseDown={() => setAnchor(null)}
      className={cn("inline-flex", className)}
    >
      {children}
      {anchor &&
        createPortal(
          <div
            ref={tipRef}
            // Hidden until placed, so it never flashes at the corner; as wide as its text
            style={{ position: "fixed", top: 0, left: 0, visibility: "hidden" }}
            className="z-9999 pointer-events-none w-max max-w-sm whitespace-pre-line text-balance bg-popover border border-border text-popover-foreground text-xs px-2 py-1 rounded-md shadow-lg"
          >
            {label}
          </div>,
          document.body,
        )}
    </div>
  );
}
