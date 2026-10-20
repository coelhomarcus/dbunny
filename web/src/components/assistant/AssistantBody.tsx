import { useState } from "react";
import { Bot, Maximize2, Minimize2, SlidersHorizontal, SquarePen, X } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import { useConnection } from "../../contexts/useConnection";
import Button from "../ui/Button";
import ClaudeCodeIcon from "./ClaudeCodeIcon";
import Tooltip from "../Tooltip";
import AssistantStatusScreen from "./AssistantStatusScreen";
import ConsentScreen from "./ConsentScreen";
import AssistantConversation from "./AssistantConversation";
import HistoryMenu from "./HistoryMenu";
import ConnectionOptionsDialog from "./ConnectionOptionsDialog";

/** The assistant, in the side panel or in a tab of its own: its header, then what the state of the agent allows. */
export default function AssistantBody({ onClose, onBack }: { onClose?: () => void; onBack?: () => void }) {
  const assistant = useAssistant();
  const { savedConnectionId } = useConnection();
  const [options, setOptions] = useState(false);
  const ready = assistant.enabled && assistant.status.state === "ready";
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-1.5 border-b border-border px-3">
        {assistant.agent.kind === "claude" ? <ClaudeCodeIcon size={15} className="shrink-0 text-[#d97757]" /> : <Bot size={15} className="shrink-0 text-muted-foreground" />}
        <span className="shrink-0 text-[13px] font-medium">{assistant.agent.kind === "custom" ? "Agent" : assistant.agent.name}</span>
        <span className="flex-1" />
        {ready && (
          <>
            <HistoryMenu />
            {savedConnectionId && (
              <Tooltip label="Options for this connection" side="bottom">
                <Button variant="ghost-muted" size="icon-xs" aria-label="Options for this connection" onClick={() => setOptions(true)}>
                  <SlidersHorizontal />
                </Button>
              </Tooltip>
            )}
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
      {savedConnectionId && options && <ConnectionOptionsDialog savedId={savedConnectionId} open onClose={() => setOptions(false)} />}
      <div className="min-h-0 flex-1">
        {!assistant.enabled ? (
          <ConsentScreen agent={assistant.agent} onEnable={assistant.enable} />
        ) : assistant.status.state !== "ready" ? (
          <AssistantStatusScreen agent={assistant.agent} status={assistant.status} onRetry={assistant.retry} onSignIn={assistant.signIn} />
        ) : (
          <AssistantConversation />
        )}
      </div>
    </div>
  );
}
