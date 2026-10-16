import { useEffect, useRef, useState, type RefObject } from "react";
import { Check } from "lucide-react";
import Popover from "../ui/Popover";
import { cn } from "../../lib/cn";

/**
 * Picker under a cell for columns with a fixed set of values (booleans, enums).
 * Picking saves right away; Escape or a click outside leaves the value alone.
 */
export default function ChoiceList({
  anchorRef,
  options,
  current,
  nullable,
  onPick,
  onCancel,
}: {
  anchorRef: RefObject<HTMLElement | null>;
  options: string[];
  /** The cell's value as text, null for NULL. */
  current: string | null;
  nullable: boolean;
  onPick: (value: string | null) => void;
  onCancel: () => void;
}) {
  const items: (string | null)[] = nullable ? [...options, null] : options;
  const [active, setActive] = useState(() => Math.max(0, items.indexOf(current)));
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current?.focus();
  }, []);

  return (
    <Popover open onClose={onCancel} anchorRef={anchorRef} className="min-w-36 p-1">
      <div
        ref={listRef}
        role="listbox"
        tabIndex={-1}
        aria-label="Values"
        className="max-h-64 overflow-y-auto outline-none"
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const step = e.key === "ArrowDown" ? 1 : -1;
            setActive((i) => (i + step + items.length) % items.length);
          } else if (e.key === "Enter") {
            e.preventDefault();
            // Handled here only: the table's own Enter/Cmd+S handling would save the stale draft
            e.stopPropagation();
            onPick(items[active]);
          }
        }}
      >
        {items.map((item, i) => (
          <button
            key={item ?? "\0null"}
            type="button"
            role="option"
            aria-selected={item === current}
            tabIndex={-1}
            onMouseEnter={() => setActive(i)}
            onClick={() => onPick(item)}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] cursor-pointer",
              i === active && "bg-accent",
              item === null && "italic text-muted-foreground",
            )}
          >
            <span className="min-w-0 flex-1 truncate">{item ?? "NULL"}</span>
            {item === current && <Check size={13} className="shrink-0 text-primary" />}
          </button>
        ))}
      </div>
    </Popover>
  );
}
