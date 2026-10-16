import { ShieldAlert } from "lucide-react";
import type { SshHostKeyProblem } from "@/types";
import ConfirmDialog from "../ui/ConfirmDialog";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";

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
  // A mismatch with ~/.ssh/known_hosts can't be accepted here: DBunny doesn't change that file
  if (request && request.problem.inUserKnownHosts) {
    return (
      <Dialog
        open
        onClose={onClose}
        icon={<ShieldAlert />}
        tone="destructive"
        title="The SSH server's key changed"
        description={`${request.problem.host} presented a different key than the one in ~/.ssh/known_hosts. Someone may be intercepting the connection. If the server was reinstalled, remove its old entry from known_hosts and try again.`}
        footer={
          <Button variant="neutral" size="sm" onClick={onClose}>
            Close
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
      title={changed ? "The SSH server's key changed" : "Trust this SSH server?"}
      description={
        changed
          ? "It's different from the key accepted before. Someone may be intercepting the connection; trust the new key only if you know the server changed."
          : "This is the first connection to this server. Check that the fingerprint matches the one your server administrator gives you."
      }
      cancelLabel="Cancel"
      confirmLabel={changed ? "Trust the new key" : "Trust and connect"}
    >
      {request && <HostKeyFingerprint problem={request.problem} />}
    </ConfirmDialog>
  );
}
