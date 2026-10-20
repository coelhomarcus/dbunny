import { useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { agentInfo, type AgentKind } from "../../lib/assistantAgents";
import { cn } from "../../lib/cn";
import AgentIcon from "../assistant/AgentIcon";
import Popover from "../ui/Popover";

const KINDS: AgentKind[] = ["claude", "codex", "custom"];

/** The agent the assistant runs: a button with its mark and name that lists the others. */
export default function AgentPicker({ value, onChange }: { value: AgentKind; onChange: (kind: AgentKind) => void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const label = (kind: AgentKind) => (kind === "custom" ? "Custom agent" : agentInfo(kind).name);
  const maker = (kind: AgentKind) => (kind === "custom" ? "Your own command" : agentInfo(kind).company);

  return (
    <div ref={anchor}>
      <button
        type="button"
        aria-label="Agent"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-9 w-full max-w-xs cursor-pointer items-center gap-2.5 rounded-lg border border-input bg-card px-3 text-left text-[13px] font-medium outline-none hover:bg-accent focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 dark:bg-muted"
      >
        <AgentIcon kind={value} />
        <span className="min-w-0 flex-1 truncate">{label(value)}</span>
        <ChevronDown size={14} className="shrink-0 text-muted-foreground" />
      </button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchor} className="w-64 p-1">
        {KINDS.map((kind) => (
          <button
            key={kind}
            type="button"
            role="menuitemradio"
            aria-checked={kind === value}
            onClick={() => {
              setOpen(false);
              if (kind !== value) onChange(kind);
            }}
            className={cn("flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-left hover:bg-accent", kind === value && "bg-accent/60")}
          >
            <AgentIcon kind={kind} size={16} />
            <span className="min-w-0 flex-1">
              <span className="block text-[13px] font-medium">{label(kind)}</span>
              <span className="block text-xs text-muted-foreground">{maker(kind)}</span>
            </span>
            {kind === value && <Check size={13} className="shrink-0" />}
          </button>
        ))}
      </Popover>
    </div>
  );
}
