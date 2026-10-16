import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";

export type ContextMenuItem =
  | { label: string; icon?: ReactNode; onSelect: () => void; disabled?: boolean; destructive?: boolean }
  | "separator";

const MARGIN = 8;

/**
 * Menu at the pointer, for right-clicks. Closes on Escape, a click elsewhere,
 * scrolling or the window losing focus; arrow keys and Enter work too.
 */
export default function ContextMenu({
  position,
  items,
  onClose,
}: {
  position: { x: number; y: number } | null;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(-1);
  const actionable = items.flatMap((item, i) => (item !== "separator" && !item.disabled ? [i] : []));

  // Opens towards the free side when there's no room right of / below the pointer
  useLayoutEffect(() => {
    const el = ref.current;
    if (!position || !el) return;
    const { width, height } = el.getBoundingClientRect();
    const left = position.x + width > window.innerWidth - MARGIN ? position.x - width : position.x;
    const top = position.y + height > window.innerHeight - MARGIN ? position.y - height : position.y;
    el.style.left = `${Math.max(MARGIN, left)}px`;
    el.style.top = `${Math.max(MARGIN, top)}px`;
    el.style.visibility = "visible";
    el.focus();
  }, [position]);

  useEffect(() => {
    if (!position) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const close = () => onClose();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("wheel", close, { passive: true });
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("wheel", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [position, onClose]);

  if (!position) return null;

  function select(index: number) {
    const item = items[index];
    if (item === "separator" || item.disabled) return;
    onClose();
    item.onSelect();
  }

  return createPortal(
    <div
      ref={ref}
      role="menu"
      tabIndex={-1}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && actionable.length) {
          e.preventDefault();
          const pos = actionable.indexOf(active);
          const next = e.key === "ArrowDown" ? pos + 1 : pos <= 0 ? actionable.length - 1 : pos - 1;
          setActive(actionable[next % actionable.length]);
        } else if (e.key === "Enter" && active >= 0) {
          e.preventDefault();
          e.stopPropagation();
          select(active);
        }
      }}
      className="fixed z-50 min-w-52 rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-xl outline-none animate-dialog-in"
      style={{ top: 0, left: 0, visibility: "hidden" }}
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <div key={`sep-${i}`} role="separator" className="mx-1 my-1 h-px bg-border" />
        ) : (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            tabIndex={-1}
            disabled={item.disabled}
            onMouseEnter={() => setActive(i)}
            onMouseLeave={() => setActive(-1)}
            onClick={() => select(i)}
            className={cn(
              "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] cursor-pointer outline-none",
              "disabled:pointer-events-none disabled:opacity-45 [&_svg]:size-3.5 [&_svg]:shrink-0",
              i === active && "bg-accent",
              item.destructive ? "text-destructive-foreground" : "[&_svg]:text-muted-foreground",
            )}
          >
            {item.icon}
            <span className="min-w-0 flex-1 truncate">{item.label}</span>
          </button>
        ),
      )}
    </div>,
    document.body,
  );
}
