import { useState, type FormEvent } from "react";
import { Database, FolderPlus, PenLine, Trash2 } from "lucide-react";
import Dialog from "../ui/Dialog";
import CheckOption from "../ui/CheckOption";
import Button from "../ui/Button";
import Input from "../ui/Input";
import { useCapabilities } from "../../hooks/useCapabilities";

export type ContainerKind = "schema" | "database";
export type ContainerAction = "create" | "rename" | "drop";

export interface ContainerRequest {
  kind: ContainerKind;
  action: ContainerAction;
  /** The schema or database acted on; empty when creating. */
  name: string;
}

export interface ContainerOptions {
  name: string;
  cascade: boolean;
  force: boolean;
}

function ContainerForm({
  request,
  onRun,
  onClose,
}: {
  request: ContainerRequest;
  /** Runs the action; a rejection is shown in the dialog. */
  onRun: (options: ContainerOptions) => Promise<void>;
  onClose: () => void;
}) {
  const { kind, action, name: target } = request;
  const [name, setName] = useState(action === "rename" ? target : "");
  const [cascade, setCascade] = useState(false);
  const [force, setForce] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const capabilities = useCapabilities();

  const destructive = action === "drop";
  // Dropping needs the name typed out, so a slip of the mouse can't cause it
  const canRun = destructive ? confirmText === target : name.trim() !== "" && name.trim() !== target;

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (!canRun || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onRun({ name: name.trim(), cascade, force });
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "It didn't work");
      setBusy(false);
    }
  }

  const config = {
    create: {
      icon: kind === "schema" ? <FolderPlus /> : <Database />,
      title: `New ${kind}`,
      button: "Create",
      description: kind === "schema" ? "An empty namespace for tables, views and functions." : "An empty database on this server. It opens in a new tab.",
    },
    rename: { icon: <PenLine />, title: `Rename ${kind}`, button: "Rename", description: target },
    drop: {
      icon: <Trash2 />,
      title: `Drop the ${kind}?`,
      button: "Drop",
      description:
        kind === "schema"
          ? `Deletes the schema ${target}. This can't be undone.`
          : `Deletes the database ${target} and everything in it. This can't be undone.`,
    },
  }[action];

  return (
    <Dialog
      open
      onClose={onClose}
      icon={config.icon}
      tone={destructive ? "destructive" : "neutral"}
      title={config.title}
      description={config.description}
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 select-text break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={destructive}>
            Cancel
          </Button>
          <Button variant={destructive ? "destructive" : "primary"} size="sm" onClick={() => submit()} disabled={!canRun || busy}>
            {busy && action === "create" ? "Creating…" : config.button}
          </Button>
        </>
      }
    >
      <form onSubmit={submit} className="flex flex-col gap-3.5">
        {!destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px] font-medium">{action === "rename" ? "New name" : "Name"}</span>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus spellCheck={false} autoComplete="off" className="font-mono" />
          </label>
        )}
        {destructive && kind === "schema" && capabilities.cascade && (
          <CheckOption checked={cascade} onChange={setCascade} label="Cascade" hint="Also drops everything in it; without it, only an empty schema can go" />
        )}
        {destructive && kind === "database" && (
          <CheckOption checked={force} onChange={setForce} label="Disconnect its sessions" hint="Ends the connections still using it instead of failing (PostgreSQL 13+)" />
        )}
        {destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px]">
              Type <code className="rounded bg-muted px-1 font-mono text-xs">{target}</code> to confirm
            </span>
            <Input value={confirmText} onChange={(e) => setConfirmText(e.target.value)} spellCheck={false} autoComplete="off" className="font-mono" />
          </label>
        )}
        <button type="submit" className="hidden" />
      </form>
    </Dialog>
  );
}

/** Create, rename or drop a schema or a database. Mounted only while open. */
export default function ContainerDialog({
  request,
  ...props
}: {
  request: ContainerRequest | null;
  onRun: (options: ContainerOptions) => Promise<void>;
  onClose: () => void;
}) {
  return request ? <ContainerForm request={request} {...props} /> : null;
}
