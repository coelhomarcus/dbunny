import { useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "../../lib/cn";
import type { ModelChoice } from "../../types/assistant";
import Popover from "../ui/Popover";

/** The model the conversation runs on: a small button under the text field that lists the others. */
export default function ModelPicker({ models, onChange }: { models: ModelChoice; onChange: (modelId: string) => void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const current = models.available.find((model) => model.id === models.current);

  return (
    <div ref={anchor}>
      <button
        type="button"
        aria-label="Model"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-5 max-w-48 cursor-pointer items-center gap-1 rounded-md px-1.5 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
      >
        <span className="truncate">{current?.name ?? models.current}</span>
        <ChevronDown size={10} className="shrink-0" />
      </button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchor} className="w-56 p-1">
        <div className="max-h-64 overflow-y-auto">
          {models.available.map((model) => (
            <button
              key={model.id}
              type="button"
              role="menuitemradio"
              aria-checked={model.id === models.current}
              onClick={() => {
                setOpen(false);
                if (model.id !== models.current) onChange(model.id);
              }}
              className={cn("flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-accent", model.id === models.current && "font-medium")}
            >
              <span className="min-w-0 flex-1 truncate">{model.name}</span>
              {model.id === models.current && <Check size={12} className="shrink-0" />}
            </button>
          ))}
        </div>
      </Popover>
    </div>
  );
}
