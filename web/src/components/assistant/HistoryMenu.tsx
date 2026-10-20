import { useRef, useState } from "react";
import { History } from "lucide-react";
import { stripContext } from "../../lib/assistantContext";
import { groupByDay } from "../../lib/assistantHistory";
import { Marker } from "../chat/Marker";
import { useAssistant } from "../../contexts/assistant";
import { useConnection } from "../../contexts/useConnection";
import Button from "../ui/Button";
import Popover from "../ui/Popover";
import Tooltip from "../Tooltip";

/** The time of day a conversation was last used; its day is the heading above it. */
function time(iso: string | null): string {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** The conversations held before with this saved connection. */
export default function HistoryMenu() {
  const assistant = useAssistant();
  const { savedConnectionId } = useConnection();
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);

  return (
    <div ref={anchor}>
      <Tooltip label="History" side="bottom">
        <Button
          variant="ghost-muted"
          size="icon-xs"
          aria-label="History"
          aria-expanded={open}
          onClick={() => {
            if (!open) assistant.refreshHistory();
            setOpen(!open);
          }}
        >
          <History />
        </Button>
      </Tooltip>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchor} className="w-72">
        <div className="max-h-80 overflow-y-auto p-1">
          {!savedConnectionId ? (
            <p className="px-2.5 py-2 text-xs text-muted-foreground">Conversations of connections you save are kept here.</p>
          ) : assistant.history.length === 0 ? (
            <p className="px-2.5 py-2 text-xs text-muted-foreground">No past conversations.</p>
          ) : (
            groupByDay(assistant.history).map((group) => (
              <div key={group.label} className="pb-1">
                <Marker variant="separator" className="px-2.5 py-1.5 text-[11px]">
                  {group.label}
                </Marker>
                {group.entries.map((entry) => (
                  <button
                    key={entry.acpSession}
                    type="button"
                    onClick={() => {
                      setOpen(false);
                      assistant.openConversation(entry.acpSession, (entry.title && stripContext(entry.title)) || "");
                    }}
                    className="flex w-full cursor-pointer items-baseline gap-2 rounded-md px-2.5 py-1.5 text-left hover:bg-accent"
                  >
                    <span className="min-w-0 flex-1 truncate text-[13px]">{(entry.title && stripContext(entry.title)) || "Untitled"}</span>
                    <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">{time(entry.updatedAt)}</span>
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      </Popover>
    </div>
  );
}
