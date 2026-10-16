import { useState, useRef } from "react";
import { createPortal } from "react-dom";
import { cn } from "../lib/cn";

interface TooltipProps {
  label: string;
  children: React.ReactNode;
  side?: "right" | "left" | "top" | "bottom";
  className?: string;
}

export default function Tooltip({ label, children, side = "right", className }: TooltipProps) {
  const [pos, setPos] = useState<React.CSSProperties | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  function show() {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const gap = 8;

    const styles: Record<string, React.CSSProperties> = {
      right: { top: r.top + r.height / 2, left: r.right + gap, transform: "translateY(-50%)" },
      left: { top: r.top + r.height / 2, right: window.innerWidth - r.left + gap, transform: "translateY(-50%)" },
      top: { bottom: window.innerHeight - r.top + gap, left: r.left + r.width / 2, transform: "translateX(-50%)" },
      bottom: { top: r.bottom + gap, left: r.left + r.width / 2, transform: "translateX(-50%)" },
    };

    setPos(styles[side]);
  }

  return (
    <div ref={ref} onMouseEnter={show} onMouseLeave={() => setPos(null)} className={cn("inline-flex", className)}>
      {children}
      {pos &&
        createPortal(
          <div
            style={{ ...pos, position: "fixed" }}
            className="z-9999 pointer-events-none max-w-80 whitespace-pre-line bg-popover border border-border text-popover-foreground text-xs px-2.5 py-1 rounded-md shadow-lg"
          >
            {label}
          </div>,
          document.body,
        )}
    </div>
  );
}
