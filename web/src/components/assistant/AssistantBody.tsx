import { Maximize2, Minimize2, SquarePen, Sparkles, X } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import { selectClass } from "../ui/styles";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import AssistantStatusScreen from "./AssistantStatusScreen";
import ConsentScreen from "./ConsentScreen";
import AssistantConversation from "./AssistantConversation";
import HistoryMenu from "./HistoryMenu";

/** The assistant, in the side panel or in a tab of its own: its header, then what the state of the agent allows. */
export default function AssistantBody({ onClose, onBack }: { onClose?: () => void; onBack?: () => void }) {
  const assistant = useAssistant();
  const ready = assistant.enabled && assistant.status.state === "ready";
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-1.5 border-b border-border px-3">
        <Sparkles size={14} className="shrink-0 text-muted-foreground" />
        <span className="shrink-0 text-[13px] font-medium">Claude Code</span>
        {ready && assistant.models && assistant.models.available.length > 0 && (
          <select
            aria-label="Model"
            value={assistant.models.current}
            onChange={(event) => assistant.setModel(event.target.value)}
            className={`${selectClass} h-6 max-w-32 truncate text-xs`}
          >
            {assistant.models.available.map((model) => (
              <option key={model.id} value={model.id}>
                {model.name}
              </option>
            ))}
          </select>
        )}
        <span className="flex-1" />
        {ready && (
          <>
            <HistoryMenu />
            <Tooltip label="New chat" side="bottom">
              <Button variant="ghost-muted" size="icon-xs" aria-label="New chat" onClick={() => assistant.newChat()}>
                <SquarePen />
              </Button>
            </Tooltip>
          </>
        )}
        {onClose && (
          <Tooltip label="Open in a tab" side="bottom">
            <Button variant="ghost-muted" size="icon-xs" aria-label="Open in a tab" onClick={assistant.expand}>
              <Maximize2 />
            </Button>
          </Tooltip>
        )}
        {onBack && (
          <Tooltip label="Back to the side panel" side="bottom">
            <Button variant="ghost-muted" size="icon-xs" aria-label="Back to the side panel" onClick={onBack}>
              <Minimize2 />
            </Button>
          </Tooltip>
        )}
        {onClose && (
          <Tooltip label="Close" side="bottom">
            <Button variant="ghost-muted" size="icon-xs" aria-label="Close" onClick={onClose}>
              <X />
            </Button>
          </Tooltip>
        )}
      </div>
      <div className="min-h-0 flex-1">
        {!assistant.enabled ? (
          <ConsentScreen onEnable={assistant.enable} />
        ) : assistant.status.state !== "ready" ? (
          <AssistantStatusScreen status={assistant.status} onRetry={assistant.retry} />
        ) : (
          <AssistantConversation />
        )}
      </div>
    </div>
  );
}
