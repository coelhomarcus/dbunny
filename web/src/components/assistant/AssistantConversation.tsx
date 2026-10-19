import { useState } from "react";
import { ArrowUp, Square } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import Button from "../ui/Button";

const SUGGESTIONS = ["Which tables hold customer data?", "How many rows does each table have?", "What changed in the last 7 days?"];

/** The conversation and where to write to the assistant. */
export default function AssistantConversation() {
  const assistant = useAssistant();
  const [draft, setDraft] = useState("");
  const { conversation } = assistant;

  function submit() {
    const text = draft.trim();
    if (!text || conversation.running) return;
    setDraft("");
    assistant.send(text);
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {conversation.items.length === 0 && (
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
        )}
      </div>
      <div className="shrink-0 border-t border-border p-2">
        <div className="flex items-end gap-2 rounded-lg border border-input bg-card px-2 py-1.5">
          <textarea
            value={draft}
            rows={1}
            placeholder="Ask about your data…"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
            }}
            className="max-h-40 min-h-6 flex-1 resize-none bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
          />
          {conversation.running ? (
            <Button variant="neutral" size="icon-xs" aria-label="Stop" onClick={() => assistant.stop()}>
              <Square />
            </Button>
          ) : (
            <Button variant="primary" size="icon-xs" aria-label="Send" disabled={!draft.trim()} onClick={submit}>
              <ArrowUp />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
