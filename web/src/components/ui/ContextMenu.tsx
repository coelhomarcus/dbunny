import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ChevronRight } from "lucide-react";
import { cn } from "../../lib/cn";

export type ContextMenuItem =
  | {
      label: string;
      icon?: ReactNode;
      /** Shown on the right, e.g. "⌘C". */
      shortcut?: string;
      onSelect?: () => void;
      /** Opens a submenu instead of running something. */
      items?: ContextMenuItem[];
      disabled?: boolean;
      destructive?: boolean;
    }
  | { heading: string }
  | "separator";

type ActionItem = Extract<ContextMenuItem, { label: string }>;

const MARGIN = 8;
/** How long the pointer rests on an item before its submenu opens. */
const SUBMENU_DELAY = 120;

const isAction = (item: ContextMenuItem): item is ActionItem => typeof item === "object" && "label" in item;

/**
 * One level of the menu: placed at a point (the root) or beside an item (a submenu),
 * always inside the window, scrolling when it's taller than the window.
 */
function MenuPanel({
  items,
  at,
  beside,
  focusOnOpen,
  onSelectDone,
  onBack,
}: {
  items: ContextMenuItem[];
  /** The pointer, for the root panel. */
  at?: { x: number; y: number };
  /** The item a submenu belongs to. */
  beside?: DOMRect;
  focusOnOpen: boolean;
  /** Something was chosen: the whole menu closes. */
  onSelectDone: () => void;
  /** Leave this submenu (ArrowLeft / Escape); undefined on the root. */
  onBack?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [active, setActive] = useState(-1);
  const [open, setOpen] = useState<{ index: number; rect: DOMRect; focus: boolean } | null>(null);
  const hoverTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const actionable = items.flatMap((item, i) => (isAction(item) && !item.disabled ? [i] : []));

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    let left: number;
    let top: number;
    if (beside) {
      // Beside its item, overlapping a little; on the left when the right has no room
      left = beside.right - 4 + width > window.innerWidth - MARGIN ? beside.left + 4 - width : beside.right - 4;
      top = beside.top - 5;
    } else {
      // Towards the free side of the pointer
      const x = at?.x ?? 0;
      const y = at?.y ?? 0;
      left = x + width > window.innerWidth - MARGIN ? x - width : x;
      top = y + height > window.innerHeight - MARGIN ? y - height : y;
    }
    el.style.left = `${Math.min(Math.max(MARGIN, left), window.innerWidth - width - MARGIN)}px`;
    el.style.top = `${Math.min(Math.max(MARGIN, top), window.innerHeight - height - MARGIN)}px`;
    el.style.visibility = "visible";
    if (focusOnOpen) el.focus();
    // Placed once, where it opened
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => () => clearTimeout(hoverTimer.current), []);

  function openSubmenu(index: number, focus: boolean) {
    const button = itemRefs.current[index];
    if (button) setOpen({ index, rect: button.getBoundingClientRect(), focus });
  }

  function choose(index: number, viaKeyboard = false) {
    const item = items[index];
    if (!isAction(item) || item.disabled) return;
    if (item.items) {
      openSubmenu(index, viaKeyboard);
      return;
    }
    onSelectDone();
    item.onSelect?.();
  }

  function move(step: 1 | -1 | "first" | "last") {
    if (!actionable.length) return;
    const pos = actionable.indexOf(active);
    const next =
      step === "first" ? 0 : step === "last" ? actionable.length - 1 : pos < 0 ? (step === 1 ? 0 : actionable.length - 1) : (pos + step + actionable.length) % actionable.length;
    setActive(actionable[next]);
    itemRefs.current[actionable[next]]?.scrollIntoView({ block: "nearest" });
  }

  const submenuItem = open ? items[open.index] : undefined;

  return (
    <>
      <div
        ref={ref}
        role="menu"
        tabIndex={-1}
        onContextMenu={(e) => e.preventDefault()}
        onKeyDown={(e) => {
          const key = e.key;
          if (["ArrowDown", "ArrowUp", "Home", "End", "Enter", " ", "ArrowRight", "ArrowLeft", "Escape"].includes(key)) {
            e.preventDefault();
            e.stopPropagation();
          }
          if (key === "ArrowDown") move(1);
          else if (key === "ArrowUp") move(-1);
          else if (key === "Home") move("first");
          else if (key === "End") move("last");
          else if ((key === "Enter" || key === " ") && active >= 0) choose(active, true);
          else if (key === "ArrowRight" && active >= 0 && isAction(items[active]) && (items[active] as ActionItem).items) openSubmenu(active, true);
          else if (key === "ArrowLeft" || key === "Escape") (onBack ?? onSelectDone)();
        }}
        className="fixed z-50 max-h-[calc(100vh-16px)] min-w-52 max-w-80 overflow-y-auto rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-xl outline-none animate-dialog-in"
        style={{ top: 0, left: 0, visibility: "hidden" }}
      >
        {items.map((item, i) => {
          if (item === "separator") return <div key={`sep-${i}`} role="separator" className="mx-1 my-1 h-px bg-border" />;
          if (!isAction(item)) {
            return (
              <div key={`heading-${i}`} className="px-2 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground select-none">
                {item.heading}
              </div>
            );
          }
          const expanded = open?.index === i;
          return (
            <button
              key={`${item.label}-${i}`}
              ref={(el) => {
                itemRefs.current[i] = el;
              }}
              type="button"
              role="menuitem"
              tabIndex={-1}
              disabled={item.disabled}
              aria-haspopup={item.items ? "menu" : undefined}
              aria-expanded={item.items ? expanded : undefined}
              onMouseEnter={() => {
                setActive(i);
                clearTimeout(hoverTimer.current);
                // Resting on an item opens its submenu, or closes another one
                hoverTimer.current = setTimeout(() => (item.items ? openSubmenu(i, false) : setOpen(null)), SUBMENU_DELAY);
              }}
              onMouseLeave={() => {
                clearTimeout(hoverTimer.current);
                if (!item.items) setActive(-1);
              }}
              onClick={() => choose(i)}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] cursor-pointer outline-none",
                "disabled:pointer-events-none disabled:opacity-45 [&_svg]:size-3.5 [&_svg]:shrink-0",
                (i === active || expanded) && "bg-accent",
                item.destructive ? "text-destructive-foreground" : "[&_svg]:text-muted-foreground",
              )}
            >
              {item.icon}
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
              {item.shortcut && <kbd className="ml-4 shrink-0 font-sans text-[11px] text-muted-foreground">{item.shortcut}</kbd>}
              {item.items && <ChevronRight className="ml-2 text-muted-foreground" />}
            </button>
          );
        })}
      </div>
      {open && submenuItem && isAction(submenuItem) && submenuItem.items && (
        <MenuPanel
          key={open.index}
          items={submenuItem.items}
          beside={open.rect}
          focusOnOpen={open.focus}
          onSelectDone={onSelectDone}
          onBack={() => {
            setOpen(null);
            ref.current?.focus();
          }}
        />
      )}
    </>
  );
}

/**
 * Menu at the pointer, for right-clicks: items with icons and shortcuts, headings,
 * separators and submenus. Stays inside the window and scrolls when it's tall. Closes on
 * Escape, a click elsewhere, scrolling outside it or the window losing focus; arrows,
 * Home/End, Enter and Space work, ArrowRight/ArrowLeft go in and out of submenus.
 */
export default function ContextMenu({
  position,
  items,
  onClose,
  focus = true,
}: {
  position: { x: number; y: number } | null;
  items: ContextMenuItem[];
  onClose: () => void;
  /** Take the keyboard focus (off for menus that act on a focused text field). */
  focus?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!position) return;
    const inside = (target: EventTarget | null) => target instanceof Node && root.current?.contains(target);
    const onPointerDown = (e: PointerEvent) => {
      if (!inside(e.target)) onClose();
    };
    const onWheel = (e: WheelEvent) => {
      if (!inside(e.target)) onClose();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      // Without focus the panels don't hear Escape themselves
      if (e.key === "Escape") onClose();
    };
    const close = () => onClose();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("wheel", onWheel, { passive: true });
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [position, onClose]);

  if (!position) return null;

  return createPortal(
    // Holds every level, so a click in a submenu counts as inside
    <div ref={root} className="contents">
      <MenuPanel key={`${position.x},${position.y}`} items={items} at={position} focusOnOpen={focus} onSelectDone={onClose} />
    </div>,
    document.body,
  );
}
