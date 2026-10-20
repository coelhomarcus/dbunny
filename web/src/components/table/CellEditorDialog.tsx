import { useRef, useState } from "react";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import { cn } from "../../lib/cn";
import { toEditText, toJsonEditText } from "../../lib/cellEdit";
import { isMac } from "../../lib/platform";
import JsonValueEditor, { type JsonValueEditorHandle } from "./JsonValueEditor";
import { Keys } from "../ui/Kbd";
import { useT } from "../../i18n";

function Editor({
  column,
  typeLabel,
  value,
  json,
  nullable,
  onApply,
  onCancel,
}: {
  column: string;
  typeLabel?: string;
  value: unknown;
  json: boolean;
  nullable: boolean;
  onApply: (isNull: boolean, text: string) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(() => (json ? toJsonEditText(value) : toEditText(value)));
  const [isNull, setIsNull] = useState(value === null || value === undefined);
  const [error, setError] = useState<string | null>(null);
  const jsonEditor = useRef<JsonValueEditorHandle>(null);

  function apply() {
    if (isNull) return onApply(true, "");
    if (!json) return onApply(false, text);
    try {
      // Saved compact; the server stores JSON its own way anyway
      onApply(false, JSON.stringify(JSON.parse(text)));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("table.cellEditorDialog.invalidJson"));
    }
  }

  function format() {
    const message = jsonEditor.current?.format();
    setError(message ?? null);
  }

  return (
    <Dialog
      open
      onClose={onCancel}
      className="max-w-2xl"
      title={
        <>
          <span className="font-mono">{column}</span>
          {typeLabel && <span className="ml-2 text-xs font-normal text-muted-foreground">{typeLabel}</span>}
        </>
      }
      footer={
        <>
          {error ? (
            <p className="mr-auto min-w-0 flex-1 select-text truncate text-xs text-destructive-foreground">{error}</p>
          ) : (
            <p className="mr-auto flex items-center gap-1.5 text-xs text-muted-foreground">
              <Keys keys={isMac ? "⌘↵" : "Ctrl+↵"} /> {t("table.cellEditorDialog.toApply")}
            </p>
          )}
          {json && (
            <Button variant="outline" size="sm" onClick={format} disabled={isNull}>
              {t("table.cellEditorDialog.format")}
            </Button>
          )}
          {nullable && (
            <Button
              variant={isNull ? "neutral" : "outline"}
              size="sm"
              aria-pressed={isNull}
              onClick={() => {
                setIsNull(true);
                setError(null);
              }}
            >
              {t("table.cellEditorDialog.setNull")}
            </Button>
          )}
          <Button variant="neutral" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" onClick={apply}>
            {t("table.cellEditorDialog.apply")}
          </Button>
        </>
      }
    >
      {json ? (
        <JsonValueEditor
          initialValue={text}
          handleRef={jsonEditor}
          onChange={(next) => {
            setText(next);
            setIsNull(false);
            setError(null);
          }}
          onApply={apply}
        />
      ) : (
        <textarea
          autoFocus
          aria-label={t("table.cellEditorDialog.valueOf", { column })}
          value={text}
          placeholder={isNull ? "NULL" : t("ui.editableCell.empty")}
          spellCheck={false}
          onChange={(e) => {
            setText(e.target.value);
            setIsNull(false);
            setError(null);
          }}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && (e.key === "Enter" || e.key === "s")) {
              e.preventDefault();
              // The table saves pending changes on Cmd+S; here it only applies this value
              e.stopPropagation();
              apply();
            }
          }}
          className={cn(
            "block h-72 w-full resize-y rounded-lg border border-input bg-card px-3 py-2 font-mono text-xs leading-relaxed text-foreground outline-none",
            "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 placeholder:italic placeholder:text-muted-foreground/70 dark:bg-muted",
            error && "border-destructive/60",
          )}
        />
      )}
    </Dialog>
  );
}

/** Larger editor for JSON and long text. Mounted only while open, so it starts from the cell's value. */
export default function CellEditorDialog({ open, ...props }: { open: boolean } & Parameters<typeof Editor>[0]) {
  return open ? <Editor {...props} /> : null;
}
