import { useState, type FormEvent } from "react";
import { Copy, PenLine, Trash2, Eraser } from "lucide-react";
import type { RelationKind } from "../../lib/api";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import Checkbox from "../Checkbox";

export type RelationAction = "rename" | "duplicate" | "truncate" | "drop";

export interface RelationTarget {
  schema: string;
  name: string;
  kind: RelationKind;
}

const KIND_LABEL: Record<RelationKind, string> = { table: "table", view: "view", matview: "materialized view" };

function Option({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint: string }) {
  return (
    // A div, not a label: a label would also click the checkbox button and toggle twice
    <div className="flex cursor-pointer items-start gap-2.5 select-none" onClick={() => onChange(!checked)}>
      <Checkbox checked={checked} onChange={() => onChange(!checked)} className="mt-0.5" />
      <span>
        <span className="block text-[13px] font-medium">{label}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </span>
    </div>
  );
}

function ActionForm({
  action,
  target,
  onRun,
  onClose,
}: {
  action: RelationAction;
  target: RelationTarget;
  /** Runs the action; a rejection is shown in the dialog. */
  onRun: (options: { newName: string; withData: boolean; cascade: boolean; restartIdentity: boolean }) => Promise<void>;
  onClose: () => void;
}) {
  const [newName, setNewName] = useState(action === "duplicate" ? `${target.name}_copy` : target.name);
  const [withData, setWithData] = useState(true);
  const [cascade, setCascade] = useState(false);
  const [restartIdentity, setRestartIdentity] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const kind = KIND_LABEL[target.kind];
  const destructive = action === "drop" || action === "truncate";
  // Destructive actions need the name typed out, so a slip of the mouse can't cause them
  const canRun = destructive ? confirmText === target.name : newName.trim() !== "" && newName.trim() !== (action === "rename" ? target.name : "");

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (!canRun || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onRun({ newName: newName.trim(), withData, cascade, restartIdentity });
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "It didn't work");
      setBusy(false);
    }
  }

  const config = {
    rename: { icon: <PenLine />, tone: "neutral" as const, title: `Rename ${kind}`, button: "Rename", description: `${target.schema}.${target.name}` },
    duplicate: { icon: <Copy />, tone: "neutral" as const, title: "Duplicate table", button: "Duplicate", description: `Copies the columns, constraints, indexes and defaults of ${target.schema}.${target.name}.` },
    truncate: { icon: <Eraser />, tone: "destructive" as const, title: "Empty the table?", button: "Truncate", description: `Deletes every row of ${target.schema}.${target.name} at once. This can't be undone.` },
    drop: { icon: <Trash2 />, tone: "destructive" as const, title: `Drop the ${kind}?`, button: "Drop", description: `Deletes ${target.schema}.${target.name}${target.kind === "table" ? " and all its rows" : ""}. This can't be undone.` },
  }[action];

  return (
    <Dialog
      open
      onClose={onClose}
      icon={config.icon}
      tone={config.tone}
      title={config.title}
      description={config.description}
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={destructive}>
            Cancel
          </Button>
          <Button variant={destructive ? "destructive" : "primary"} size="sm" onClick={() => submit()} disabled={!canRun || busy}>
            {config.button}
          </Button>
        </>
      }
    >
      <form onSubmit={submit} className="flex flex-col gap-3.5">
        {(action === "rename" || action === "duplicate") && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px] font-medium">{action === "rename" ? "New name" : "Name of the copy"}</span>
            <Input value={newName} onChange={(e) => setNewName(e.target.value)} autoFocus spellCheck={false} className="font-mono" />
          </label>
        )}
        {action === "duplicate" && (
          <Option checked={withData} onChange={setWithData} label="Copy the rows too" hint="Otherwise the copy starts empty" />
        )}
        {action === "truncate" && (
          <Option checked={restartIdentity} onChange={setRestartIdentity} label="Restart identity" hint="Serial and identity columns start counting from 1 again" />
        )}
        {destructive && (
          <Option
            checked={cascade}
            onChange={setCascade}
            label="Cascade"
            hint={action === "drop" ? "Also drops views and foreign keys that depend on it" : "Also empties tables that reference it"}
          />
        )}
        {destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px]">
              Type <code className="rounded bg-muted px-1 font-mono text-xs">{target.name}</code> to confirm
            </span>
            <Input value={confirmText} onChange={(e) => setConfirmText(e.target.value)} spellCheck={false} autoComplete="off" className="font-mono" />
          </label>
        )}
        <button type="submit" className="hidden" />
      </form>
    </Dialog>
  );
}

/** Rename, duplicate, truncate or drop a table or view. Mounted only while open. */
export default function RelationActionDialog({
  action,
  target,
  ...props
}: {
  action: RelationAction | null;
  target: RelationTarget | null;
  onRun: (options: { newName: string; withData: boolean; cascade: boolean; restartIdentity: boolean }) => Promise<void>;
  onClose: () => void;
}) {
  return action && target ? <ActionForm action={action} target={target} {...props} /> : null;
}
