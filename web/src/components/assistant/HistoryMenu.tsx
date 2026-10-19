import { useRef, useState } from "react";
import { History } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import { useConnection } from "../../contexts/useConnection";
import Button from "../ui/Button";
import Popover from "../ui/Popover";
import Tooltip from "../Tooltip";

function when(iso: string | null): string {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" }) + " " + date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
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
            assistant.history.map((entry) => (
              <button
                key={entry.acpSession}
                type="button"
                onClick={() => {
                  setOpen(false);
                  assistant.openConversation(entry.acpSession);
                }}
                className="flex w-full cursor-pointer flex-col rounded-md px-2.5 py-1.5 text-left hover:bg-accent"
              >
                <span className="truncate text-[13px]">{entry.title ?? "Untitled"}</span>
                <span className="text-[11px] text-muted-foreground">{when(entry.updatedAt)}</span>
              </button>
            ))
          )}
        </div>
      </Popover>
    </div>
  );
}
