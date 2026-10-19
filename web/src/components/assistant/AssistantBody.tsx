import { Sparkles, X } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import AssistantStatusScreen from "./AssistantStatusScreen";
import ConsentScreen from "./ConsentScreen";
import AssistantConversation from "./AssistantConversation";

/** The assistant, in the side panel or in a tab of its own: its header, then what the state of the agent allows. */
export default function AssistantBody({ onClose }: { onClose?: () => void }) {
  const assistant = useAssistant();
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3">
        <Sparkles size={14} className="text-muted-foreground" />
        <span className="text-[13px] font-medium">Claude Code</span>
        <span className="flex-1" />
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
