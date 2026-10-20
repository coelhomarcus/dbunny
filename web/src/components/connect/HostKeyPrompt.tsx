import { ShieldAlert } from "lucide-react";
import type { SshHostKeyProblem } from "@/types";
import ConfirmDialog from "../ui/ConfirmDialog";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import { useT } from "../../i18n";

/** An SSH server key waiting for the user's decision, and how to retry once trusted. */
export interface HostKeyRequest {
  problem: SshHostKeyProblem;
  retry: () => void;
}

function HostKeyFingerprint({ problem }: { problem: SshHostKeyProblem }) {
  return (
    <div className="rounded-lg border border-border bg-muted px-3 py-2.5">
      <div className="text-xs text-muted-foreground">
        {problem.host}:{problem.port} · {problem.algorithm}
      </div>
      <code className="mt-1 block break-all font-mono text-xs">{problem.fingerprint}</code>
    </div>
  );
}

/** The SSH server's key: first time, or different from before. */
export default function HostKeyPrompt({ request, onClose }: { request: HostKeyRequest | null; onClose: () => void }) {
  const t = useT();
  // A mismatch with ~/.ssh/known_hosts can't be accepted here: DBunny doesn't change that file
  if (request && request.problem.inUserKnownHosts) {
    return (
      <Dialog
        open
        onClose={onClose}
        icon={<ShieldAlert />}
        tone="destructive"
        title={t("connect.hostKeyPrompt.keyChanged")}
        description={t("connect.hostKeyPrompt.mismatch", { host: request.problem.host })}
        footer={
          <Button variant="neutral" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        }
      >
        <HostKeyFingerprint problem={request.problem} />
      </Dialog>
    );
  }

  const changed = request?.problem.status === "changed";
  return (
    <ConfirmDialog
      open={request !== null}
      onClose={onClose}
      onConfirm={() => request?.retry()}
      icon={<ShieldAlert />}
      tone={changed ? "destructive" : "warning"}
      title={changed ? t("connect.hostKeyPrompt.keyChanged") : t("connect.hostKeyPrompt.trustThisServer")}
      description={
        changed
          ? t("connect.hostKeyPrompt.changedNote")
          : t("connect.hostKeyPrompt.firstTimeNote")
      }
      cancelLabel={t("common.cancel")}
      confirmLabel={changed ? t("connect.hostKeyPrompt.trustNewKey") : t("connect.hostKeyPrompt.trustAndConnect")}
    >
      {request && <HostKeyFingerprint problem={request.problem} />}
    </ConfirmDialog>
  );
}
