import { useEffect, useRef, useState } from "react";
import { ArrowUp, Square } from "lucide-react";
import type { ModelChoice } from "../../types/assistant";
import Button from "../ui/Button";
import ModelPicker from "./ModelPicker";

/** Where the person writes to the assistant. Enter sends; Shift+Enter starts a new line. */
export default function Composer({
  running,
  models,
  onSend,
  onStop,
  onModel,
}: {
  running: boolean;
  /** The models the conversation can run on, once it has started. */
  models: ModelChoice | null;
  onSend: (text: string) => void;
  onStop: () => void;
  onModel: (modelId: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);

  // Grows with the text, up to about eight lines
  useEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [draft]);

  function submit() {
    const text = draft.trim();
    if (!text || running) return;
    setDraft("");
    onSend(text);
  }

  return (
    <div className="shrink-0 border-t border-border p-2">
      <div className="flex items-end gap-2 rounded-lg border border-input bg-card px-2 py-1.5">
        <textarea
          ref={field}
          value={draft}
          rows={1}
          placeholder="Ask about your data…"
          disabled={running}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              submit();
            }
          }}
          className="min-h-6 flex-1 resize-none bg-transparent text-[13px] leading-6 outline-none placeholder:text-muted-foreground disabled:opacity-60"
        />
        {running ? (
          <Button variant="neutral" size="icon-xs" aria-label="Stop" onClick={onStop}>
            <Square />
          </Button>
        ) : (
          <Button variant="primary" size="icon-xs" aria-label="Send" disabled={!draft.trim()} onClick={submit}>
            <ArrowUp />
          </Button>
        )}
      </div>
      {models && models.available.length > 0 && (
        <div className="flex px-0.5 pt-1">
          <ModelPicker models={models} onChange={onModel} />
        </div>
      )}
    </div>
  );
}
