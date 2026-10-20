import { useRef, useState } from "react";
import { BookOpenText, Gauge } from "lucide-react";
import AiIcon from "../ui/AiIcon";
import { useAssistant } from "../../contexts/assistant";
import Button from "../ui/Button";
import Popover from "../ui/Popover";
import Tooltip from "../Tooltip";

/** Ask the assistant about the statement in the editor: what it does, or how to make it faster. */
export default function AssistantActions({ onExplain, onOptimize }: { onExplain: () => void; onOptimize: () => void }) {
  const assistant = useAssistant();
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  if (!assistant.available || !assistant.enabled) return null;

  function choose(action: () => void) {
    setOpen(false);
    action();
  }

  return (
    <div ref={anchor}>
      <Tooltip label="Ask the assistant" side="top">
        <Button variant="ghost-muted" size="icon-sm" aria-label="Ask the assistant" aria-expanded={open} onClick={() => setOpen(!open)}>
          <AiIcon />
        </Button>
      </Tooltip>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchor} className="w-56 p-1">
        <button
          type="button"
          role="menuitem"
          onClick={() => choose(onExplain)}
          className="flex w-full cursor-pointer items-start gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-accent [&_svg]:mt-0.5 [&_svg]:size-3.5 [&_svg]:shrink-0 [&_svg]:text-muted-foreground"
        >
          <BookOpenText />
          <span className="min-w-0 flex-1">
            <span className="block text-[13px]">Explain</span>
            <span className="block text-xs text-muted-foreground">What the statement does</span>
          </span>
        </button>
        <button
          type="button"
          role="menuitem"
          onClick={() => choose(onOptimize)}
          className="flex w-full cursor-pointer items-start gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-accent [&_svg]:mt-0.5 [&_svg]:size-3.5 [&_svg]:shrink-0 [&_svg]:text-muted-foreground"
        >
          <Gauge />
          <span className="min-w-0 flex-1">
            <span className="block text-[13px]">Optimize</span>
            <span className="block text-xs text-muted-foreground">Read its plan, suggest changes</span>
          </span>
        </button>
      </Popover>
    </div>
  );
}
