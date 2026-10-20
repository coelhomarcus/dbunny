import { useRef, useState } from "react";
import { ChevronDown, Play, Workflow, Gauge } from "lucide-react";
import Button from "../ui/Button";
import { Keys } from "../ui/Kbd";
import Popover from "../ui/Popover";
import { isMac } from "../../lib/platform";
import type { RunScope } from "../SqlEditor";
import { useT, t } from "../../i18n";

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
      <span className="min-w-0 flex-1 pr-4">
        <span className="block text-[13px]">{label}</span>
        {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      </span>
      {shortcut && <Keys keys={shortcut} className="shrink-0" />}
    </button>
  );
}

/**
 * Run splits in two: the button runs the statement at the cursor (or the selection),
 * the arrow offers running everything and the EXPLAIN variants.
 */
export default function RunButton({
  disabled,
  explainAnalyze,
  analyzeHint = t("query.runButton.analyzeHint"),
  onRun,
  onExplain,
}: {
  disabled: boolean;
  /** What Explain analyze does to the statement. */
  analyzeHint?: string;
  /** The engine can run a statement to time it (EXPLAIN ANALYZE). */
  explainAnalyze: boolean;
  onRun: (scope: RunScope) => void;
  onExplain: (analyze: boolean) => void;
}) {
  const t = useT();
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
        {t("common.run")}
        <Keys keys={`${MOD}↵`} tone="inverse" className="ml-1" />
      </Button>
      <Button
        ref={arrowRef}
        variant="primary"
        size="sm"
        className="rounded-l-none border-l-primary-foreground/25 px-1.5"
        onClick={() => setOpen((v) => !v)}
        disabled={disabled}
        aria-label={t("query.runButton.moreWaysToRun")}
        aria-expanded={open}
      >
        <ChevronDown />
      </Button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={arrowRef} className="w-max min-w-64 max-w-[min(28rem,calc(100vw-2rem))] p-1">
        <div role="menu">
          <MenuItem icon={<Play />} label={t("query.runButton.runStatement")} hint={t("query.runButton.theSelectionOrTheStatement")} shortcut={`${MOD}↵`} onClick={pick(() => onRun("current"))} />
          <MenuItem icon={<Play />} label={t("query.runButton.runAll")} hint={t("query.runButton.everyStatementInTheEditor")} shortcut={`${SHIFT}${MOD}↵`} onClick={pick(() => onRun("all"))} />
          <div className="mx-1 my-1 h-px bg-border" />
          <MenuItem icon={<Workflow />} label={t("query.runButton.explain")} hint={t("query.runButton.thePlannersEstimateNothingRuns")} onClick={pick(() => onExplain(false))} />
          {explainAnalyze && (
            <MenuItem icon={<Gauge />} label={t("query.runButton.explainAnalyze")} hint={analyzeHint} onClick={pick(() => onExplain(true))} />
          )}
        </div>
      </Popover>
    </div>
  );
}
