import { useState, type FormEvent } from "react";
import { Copy, PenLine, Trash2, Eraser } from "lucide-react";
import type { RelationKind } from "../../lib/api";
import Dialog from "../ui/Dialog";
import CheckOption from "../ui/CheckOption";
import Button from "../ui/Button";
import Input from "../ui/Input";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useNotify } from "../../contexts/NoticeProvider";
import { useT, type StaticKey } from "../../i18n";
import { Rich } from "../../i18n/Rich";
import { errorText } from "../../lib/appError";

export type RelationAction = "rename" | "duplicate" | "truncate" | "drop";

export interface RelationTarget {
  schema: string;
  name: string;
  kind: RelationKind;
}

const RENAME_TITLE: Record<RelationKind, StaticKey> = { table: "schema.relationActionDialog.renameTable", view: "schema.relationActionDialog.renameView", matview: "schema.relationActionDialog.renameMatview" };
const DROP_TITLE: Record<RelationKind, StaticKey> = { table: "schema.relationActionDialog.dropTable", view: "schema.relationActionDialog.dropView", matview: "schema.relationActionDialog.dropMatview" };

function ActionForm({
  action,
  target,
  onRun,
  onClose,
}: {
  action: RelationAction;
  target: RelationTarget;
  /** Runs the action; a rejection is reported as a toast. */
  onRun: (options: { newName: string; withData: boolean; cascade: boolean; restartIdentity: boolean }) => Promise<void>;
  onClose: () => void;
}) {
  const t = useT();
  const [newName, setNewName] = useState(action === "duplicate" ? `${target.name}_copy` : target.name);
  const [withData, setWithData] = useState(true);
  const [cascade, setCascade] = useState(false);
  const [restartIdentity, setRestartIdentity] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [busy, setBusy] = useState(false);
  const notify = useNotify();
  const capabilities = useCapabilities();

  const destructive = action === "drop" || action === "truncate";
  // Destructive actions need the name typed out, so a slip of the mouse can't cause them
  const canRun = destructive ? confirmText === target.name : newName.trim() !== "" && newName.trim() !== (action === "rename" ? target.name : "");

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (!canRun || busy) return;
    setBusy(true);
    try {
      await onRun({ newName: newName.trim(), withData, cascade, restartIdentity });
      onClose();
    } catch (err) {
      notify({ tone: "error", text: errorText(err, t("schema.relationActionDialog.failed")) });
      setBusy(false);
    }
  }

  const config = {
    rename: { icon: <PenLine />, tone: "neutral" as const, title: t(RENAME_TITLE[target.kind]), button: t("common.rename"), description: `${target.schema}.${target.name}` },
    duplicate: { icon: <Copy />, tone: "neutral" as const, title: t("schema.relationActionDialog.duplicateTable"), button: t("common.duplicate"), description: t("schema.relationActionDialog.duplicateNote", { name: `${target.schema}.${target.name}` }) },
    truncate: { icon: <Eraser />, tone: "destructive" as const, title: t("schema.relationActionDialog.emptyTheTable"), button: t("schema.relationActionDialog.truncate"), description: t("schema.relationActionDialog.truncateNote", { name: `${target.schema}.${target.name}` }) },
    drop: { icon: <Trash2 />, tone: "destructive" as const, title: t(DROP_TITLE[target.kind]), button: t("schema.relationActionDialog.drop"), description: t(target.kind === "table" ? "schema.relationActionDialog.dropTableNote" : "schema.relationActionDialog.dropNote", { name: `${target.schema}.${target.name}` }) },
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
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={destructive}>
            {t("common.cancel")}
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
            <span className="text-[13px] font-medium">{action === "rename" ? t("schema.relationActionDialog.newName") : t("schema.relationActionDialog.nameOfTheCopy")}</span>
            <Input value={newName} onChange={(e) => setNewName(e.target.value)} autoFocus spellCheck={false} className="font-mono" />
          </label>
        )}
        {action === "duplicate" && (
          <CheckOption checked={withData} onChange={setWithData} label={t("schema.relationActionDialog.copyTheRowsToo")} hint={t("schema.relationActionDialog.otherwiseTheCopyStartsEmpty")} />
        )}
        {action === "truncate" && capabilities.truncateOptions && (
          <CheckOption checked={restartIdentity} onChange={setRestartIdentity} label={t("schema.relationActionDialog.restartIdentity")} hint={t("schema.relationActionDialog.serialAndIdentityColumnsStart")} />
        )}
        {destructive && capabilities.cascade && (action === "drop" || capabilities.truncateOptions) && (
          <CheckOption
            checked={cascade}
            onChange={setCascade}
            label={t("schema.relationActionDialog.cascade")}
            hint={action === "drop" ? t("schema.relationActionDialog.cascadeDropHint") : t("schema.relationActionDialog.cascadeTruncateHint")}
          />
        )}
        {destructive && (
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px]">
              <Rich id="schema.relationActionDialog.typeToConfirm" params={{ target: target.name }} tags={{ code: (text) => <code className="rounded bg-muted px-1 font-mono text-xs">{text}</code> }} />
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
