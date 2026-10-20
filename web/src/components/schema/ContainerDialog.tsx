import { useState, type FormEvent } from "react";
import { Database, FolderPlus, PenLine, Trash2 } from "lucide-react";
import Dialog from "../ui/Dialog";
import CheckOption from "../ui/CheckOption";
import Button from "../ui/Button";
import Input from "../ui/Input";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useNotify } from "../../contexts/NoticeProvider";
import { useT } from "../../i18n";
import { Rich } from "../../i18n/Rich";

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
  /** Runs the action; a rejection is reported as a toast. */
  onRun: (options: ContainerOptions) => Promise<void>;
  onClose: () => void;
}) {
  const t = useT();
  const { kind, action, name: target } = request;
  const [name, setName] = useState(action === "rename" ? target : "");
  const [cascade, setCascade] = useState(false);
  const [force, setForce] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [busy, setBusy] = useState(false);
  const notify = useNotify();
  const capabilities = useCapabilities();

  const destructive = action === "drop";
  // Dropping needs the name typed out, so a slip of the mouse can't cause it
  const canRun = destructive ? confirmText === target : name.trim() !== "" && name.trim() !== target;

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (!canRun || busy) return;
    setBusy(true);
    try {
      await onRun({ name: name.trim(), cascade, force });
      onClose();
    } catch (err) {
      notify({ tone: "error", text: typeof err === "string" ? err : err instanceof Error ? err.message : t("schema.containerDialog.failed") });
      setBusy(false);
    }
  }

  const config = {
    create: {
      icon: kind === "schema" ? <FolderPlus /> : <Database />,
      title: kind === "schema" ? t("schema.containerDialog.newSchema") : t("schema.containerDialog.newDatabase"),
      button: t("schema.containerDialog.create"),
      description: kind === "schema" ? t("schema.containerDialog.newSchemaNote") : t("schema.containerDialog.newDatabaseNote"),
    },
    rename: { icon: <PenLine />, title: kind === "schema" ? t("schema.containerDialog.renameSchema") : t("schema.containerDialog.renameDatabase"), button: t("common.rename"), description: target },
    drop: {
      icon: <Trash2 />,
      title: kind === "schema" ? t("schema.containerDialog.dropSchema") : t("schema.containerDialog.dropDatabase"),
      button: t("schema.containerDialog.drop"),
      description:
        kind === "schema"
          ? t("schema.containerDialog.dropSchemaNote", { target })
          : t("schema.containerDialog.dropDatabaseNote", { target }),
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
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={destructive}>
            {t("common.cancel")}
          </Button>
          <Button variant={destructive ? "destructive" : "primary"} size="sm" onClick={() => submit()} disabled={!canRun || busy}>
            {busy && action === "create" ? t("schema.containerDialog.creating") : config.button}
          </Button>
        </>
      }
    >
      <form onSubmit={submit} className="flex flex-col gap-3.5">
        {!destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px] font-medium">{action === "rename" ? t("schema.containerDialog.newName") : t("schema.containerDialog.name")}</span>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus spellCheck={false} autoComplete="off" className="font-mono" />
          </label>
        )}
        {destructive && kind === "schema" && capabilities.cascade && (
          <CheckOption checked={cascade} onChange={setCascade} label={t("schema.containerDialog.cascade")} hint={t("schema.containerDialog.alsoDropsEverythingInIt")} />
        )}
        {destructive && kind === "database" && (
          <CheckOption checked={force} onChange={setForce} label={t("schema.containerDialog.disconnectItsSessions")} hint={t("schema.containerDialog.endsTheConnectionsStillUsing")} />
        )}
        {destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px]">
              <Rich id="schema.containerDialog.typeToConfirm" params={{ target }} tags={{ code: (text) => <code className="rounded bg-muted px-1 font-mono text-xs">{text}</code> }} />
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
