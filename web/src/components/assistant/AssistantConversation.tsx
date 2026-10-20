import { useAssistant } from "../../contexts/assistant";
import Composer from "./Composer";
import OpeningConversation from "./OpeningConversation";
import MessageList from "./MessageList";
import SqlBlock from "./SqlBlock";
import { useT, type StaticKey } from "../../i18n";

/** A fenced block that is SQL (or says nothing of what it is) gets the buttons to use it. */
function renderCode(language: string, text: string) {
  return language === "" || /^(sql|mysql|postgres|postgresql|pgsql|sqlite)$/i.test(language) ? <SqlBlock sql={text} /> : undefined;
}

const SUGGESTIONS: StaticKey[] = ["assistant.suggestions.customers", "assistant.suggestions.rowCounts", "assistant.suggestions.changed"];

/** The conversation and where to write to the assistant. */
export default function AssistantConversation() {
  const t = useT();
  const assistant = useAssistant();
  const { conversation } = assistant;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {assistant.openingTitle !== null ? (
        <OpeningConversation title={assistant.openingTitle} />
      ) : conversation.items.length === 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
          <div className="flex flex-col gap-2 pt-6">
            {SUGGESTIONS.map((suggestion) => (
              <button
                key={suggestion}
                type="button"
                onClick={() => assistant.send(t(suggestion))}
                className="cursor-pointer rounded-lg border border-border px-3 py-2 text-left text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground"
              >
                {t(suggestion)}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <MessageList conversation={conversation} onDecide={assistant.approve} renderCode={renderCode} />
      )}
      <Composer running={conversation.running} locked={assistant.openingTitle !== null} models={assistant.models} onSend={assistant.send} onStop={assistant.stop} onModel={assistant.setModel} />
    </div>
  );
}
