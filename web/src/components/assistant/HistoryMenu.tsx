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
import { useT, fmt } from "../../i18n";

/** The time of day a conversation was last used; its day is the heading above it. */
function time(iso: string | null): string {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.getTime())) return "";
  return fmt().time(date);
}

/** The conversations held before with this saved connection. */
export default function HistoryMenu() {
  const t = useT();
  const assistant = useAssistant();
  const { savedConnectionId } = useConnection();
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);

  return (
    <div ref={anchor}>
      <Tooltip label={t("assistant.historyMenu.history")} side="bottom">
        <Button
          variant="ghost-muted"
          size="icon-xs"
          aria-label={t("assistant.historyMenu.history")}
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
            <p className="px-2.5 py-2 text-xs text-muted-foreground">{t("assistant.historyMenu.conversationsOfConnectionsYouSave")}</p>
          ) : assistant.history.length === 0 ? (
            <p className="px-2.5 py-2 text-xs text-muted-foreground">{t("assistant.historyMenu.noPastConversations")}</p>
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
                    <span className="min-w-0 flex-1 truncate text-[13px]">{(entry.title && stripContext(entry.title)) || t("assistant.historyMenu.untitled")}</span>
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
