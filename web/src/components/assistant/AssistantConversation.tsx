import { useAssistant } from "../../contexts/assistant";
import Composer from "./Composer";
import MessageList from "./MessageList";
import SqlBlock from "./SqlBlock";

/** A fenced block that is SQL (or says nothing of what it is) gets the buttons to use it. */
function renderCode(language: string, text: string) {
  return language === "" || /^(sql|mysql|postgres|postgresql|pgsql|sqlite)$/i.test(language) ? <SqlBlock sql={text} /> : undefined;
}

const SUGGESTIONS = ["Which tables hold customer data?", "How many rows does each table have?", "What changed in the last 7 days?"];

/** The conversation and where to write to the assistant. */
export default function AssistantConversation() {
  const assistant = useAssistant();
  const { conversation } = assistant;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {conversation.items.length === 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
          <div className="flex flex-col gap-2 pt-6">
            {SUGGESTIONS.map((suggestion) => (
              <button
                key={suggestion}
                type="button"
                onClick={() => assistant.send(suggestion)}
                className="cursor-pointer rounded-lg border border-border px-3 py-2 text-left text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground"
              >
                {suggestion}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <MessageList conversation={conversation} onDecide={assistant.approve} renderCode={renderCode} />
      )}
      <Composer running={conversation.running} models={assistant.models} onSend={assistant.send} onStop={assistant.stop} onModel={assistant.setModel} />
    </div>
  );
}
