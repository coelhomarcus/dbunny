import { useRef, useState } from "react";
import { ChevronDown, Play, Workflow, Gauge } from "lucide-react";
import Button from "../ui/Button";
import Popover from "../ui/Popover";
import { isMac } from "../../lib/platform";
import type { RunScope } from "../SqlEditor";

const MOD = isMac ? "⌘" : "Ctrl+";
const SHIFT = isMac ? "⇧" : "Shift+";

function MenuItem({ icon, label, hint, shortcut, onClick }: { icon: React.ReactNode; label: string; hint?: string; shortcut?: string; onClick: () => void }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left cursor-pointer hover:bg-accent [&_svg]:mt-0.5 [&_svg]:size-3.5 [&_svg]:shrink-0 [&_svg]:text-muted-foreground"
    >
      {icon}
      <span className="min-w-0 flex-1">
        <span className="block text-[13px]">{label}</span>
        {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      </span>
      {shortcut && <kbd className="shrink-0 font-sans text-[11px] text-muted-foreground">{shortcut}</kbd>}
    </button>
  );
}

/**
 * Run splits in two: the button runs the statement at the cursor (or the selection),
 * the arrow offers running everything and the EXPLAIN variants.
 */
export default function RunButton({
  disabled,
  onRun,
  onExplain,
}: {
  disabled: boolean;
  onRun: (scope: RunScope) => void;
  onExplain: (analyze: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const arrowRef = useRef<HTMLButtonElement>(null);
  const pick = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <div className="flex">
      <Button variant="primary" size="sm" className="rounded-r-none pr-2.5" onClick={() => onRun("current")} disabled={disabled}>
        <Play className="fill-current" />
        Run
        <kbd className="ml-0.5 font-sans text-[11px] opacity-70">{MOD}↵</kbd>
      </Button>
      <Button
        ref={arrowRef}
        variant="primary"
        size="sm"
        className="rounded-l-none border-l-primary-foreground/25 px-1.5"
        onClick={() => setOpen((v) => !v)}
        disabled={disabled}
        aria-label="More ways to run"
        aria-expanded={open}
      >
        <ChevronDown />
      </Button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={arrowRef} className="w-72 p-1">
        <div role="menu">
          <MenuItem icon={<Play />} label="Run statement" hint="The selection, or the statement at the cursor" shortcut={`${MOD}↵`} onClick={pick(() => onRun("current"))} />
          <MenuItem icon={<Play />} label="Run all" hint="Every statement in the editor, in order" shortcut={`${SHIFT}${MOD}↵`} onClick={pick(() => onRun("all"))} />
          <div className="mx-1 my-1 h-px bg-border" />
          <MenuItem icon={<Workflow />} label="Explain" hint="The planner's estimate; nothing runs" onClick={pick(() => onExplain(false))} />
          <MenuItem icon={<Gauge />} label="Explain analyze" hint="Runs it for real times, then rolls it back" onClick={pick(() => onExplain(true))} />
        </div>
      </Popover>
    </div>
  );
}
