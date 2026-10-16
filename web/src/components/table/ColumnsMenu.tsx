import { useRef, useState } from "react";
import { ArrowDown, ArrowUp, Columns3 } from "lucide-react";
import type { ColumnLayoutApi } from "../../hooks/useColumnLayout";
import Button from "../ui/Button";
import Popover from "../ui/Popover";
import Checkbox from "../Checkbox";
import { cn } from "../../lib/cn";

/** Show, hide and reorder the table's columns. */
export default function ColumnsMenu({ layout }: { layout: ColumnLayoutApi }) {
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const last = layout.ordered.length - 1;

  return (
    <>
      <Button
        ref={anchorRef}
        variant="ghost-muted"
        size="sm"
        data-active={open || layout.hiddenCount > 0}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <Columns3 />
        Columns
        {layout.hiddenCount > 0 && (
          <span className="rounded bg-primary/15 px-1 text-[11px] font-medium text-primary tabular-nums">{layout.hiddenCount} hidden</span>
        )}
      </Button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} className="w-64 p-1">
        <div className="max-h-80 overflow-y-auto">
          {layout.ordered.map(({ column }, i) => {
            const hidden = layout.isHidden(column.name);
            return (
              <div key={column.name} className="group flex items-center gap-2 rounded-md px-2 py-1 hover:bg-accent">
                <div className="flex min-w-0 flex-1 cursor-pointer items-center gap-2" onClick={() => layout.toggle(column.name)}>
                  <Checkbox checked={!hidden} onChange={() => layout.toggle(column.name)} />
                  <span className={cn("truncate font-mono text-xs", hidden && "text-muted-foreground")}>{column.name}</span>
                </div>
                <Button variant="ghost-muted" size="icon-xs" aria-label={`Move ${column.name} up`} disabled={i === 0} onClick={() => layout.move(column.name, -1)} className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 disabled:opacity-0">
                  <ArrowUp />
                </Button>
                <Button variant="ghost-muted" size="icon-xs" aria-label={`Move ${column.name} down`} disabled={i === last} onClick={() => layout.move(column.name, 1)} className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 disabled:opacity-0">
                  <ArrowDown />
                </Button>
              </div>
            );
          })}
        </div>
        {layout.isCustom && (
          <div className="mt-1 border-t border-border px-1 pt-1">
            <Button variant="ghost-muted" size="xs" className="w-full" onClick={layout.reset}>
              Show all in the original order
            </Button>
          </div>
        )}
      </Popover>
    </>
  );
}
