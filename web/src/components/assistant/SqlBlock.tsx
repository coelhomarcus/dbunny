import { useState } from "react";
import { Check, Copy, FilePlus2, TextCursorInput, Replace } from "lucide-react";
import { useAssistant } from "../../contexts/assistant";
import { useDocuments } from "../../contexts/DocumentsContext";
import { writeClipboardText } from "../../lib/clipboard";
import { useActiveEditor } from "../../lib/editorBridge";
import Button from "../ui/Button";
import SqlSnippet from "./SqlSnippet";
import { useT } from "../../i18n";

/** A statement the assistant wrote, and what to do with it: into the editor, a new tab, the clipboard. */
export default function SqlBlock({ sql }: { sql: string }) {
  const t = useT();
  const editor = useActiveEditor();
  const { openDocument } = useDocuments();
  const assistant = useAssistant();
  const [copied, setCopied] = useState(false);
  // The selection changes without anything here knowing, so it is looked at when the pointer comes near
  const [, look] = useState(0);

  async function copy() {
    await writeClipboardText(sql);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-card" onMouseEnter={() => look((n) => n + 1)}>
      <SqlSnippet sql={sql} />
      <div className="flex flex-wrap gap-1 border-t border-border bg-muted px-1.5 py-1">
        {editor && (
          <Button variant="ghost-muted" size="xs" onClick={() => editor.insert(sql)}>
            <TextCursorInput />
            {t("assistant.sqlBlock.insertAtCursor")}
          </Button>
        )}
        {editor?.hasSelection() && (
          <Button variant="ghost-muted" size="xs" onClick={() => editor.replaceSelection(sql)}>
            <Replace />
            {t("assistant.sqlBlock.replaceSelection")}
          </Button>
        )}
        <Button
          variant="ghost-muted"
          size="xs"
          onClick={() => {
            openDocument({ kind: "query", sql });
            // A new query tab is in front: the conversation stays where it was
            if (assistant.inTab) assistant.backToPanel();
          }}
        >
          <FilePlus2 />
          {t("assistant.sqlBlock.openInNewTab")}
        </Button>
        <Button variant="ghost-muted" size="xs" onClick={copy}>
          {copied ? <Check /> : <Copy />}
          {copied ? t("common.copied") : t("common.copy")}
        </Button>
      </div>
    </div>
  );
}
