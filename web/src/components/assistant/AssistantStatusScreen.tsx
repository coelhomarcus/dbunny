import { useState } from "react";
import { Check, Copy, Loader2, TriangleAlert } from "lucide-react";
import { assistantApi } from "../../lib/api";
import { writeClipboardText } from "../../lib/clipboard";
import { openExternal } from "../../lib/openExternal";
import type { AgentInfo } from "../../lib/assistantAgents";
import type { AgentStatus } from "../../types/assistant";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";

/** The assistant's agent is not ready: what is missing, and what to do about it. */
export default function AssistantStatusScreen({
  agent,
  status,
  onRetry,
  onSignIn,
}: {
  agent: AgentInfo;
  status: AgentStatus;
  onRetry: () => void;
  onSignIn: (methodId: string) => Promise<void>;
}) {
  const [log, setLog] = useState<string[] | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [signInError, setSignInError] = useState<string | null>(null);

  async function signIn(methodId: string) {
    setSigningIn(true);
    setSignInError(null);
    try {
      await onSignIn(methodId);
    } catch (error) {
      setSignInError(typeof error === "string" ? error : error instanceof Error ? error.message : "The login failed");
    } finally {
      setSigningIn(false);
    }
  }
  const [copied, setCopied] = useState(false);

  async function showLog() {
    setLog(await assistantApi.log().catch(() => []));
  }

  if (status.state === "idle" || status.state === "starting" || status.state === "ready") {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-[13px] text-muted-foreground">
        <Loader2 size={15} className="animate-spin" />
        Starting the assistant…
      </div>
    );
  }

  let title: string;
  let detail: string | null = null;
  let command: string | null = null;
  const actions: { label: string; run: () => void; primary?: boolean }[] = [];
  switch (status.state) {
    case "needsNode":
      title = "Node.js 20 or newer is needed";
      detail = status.found ? `Found ${status.found}` : null;
      actions.push({ label: "Get Node.js", run: () => openExternal("https://nodejs.org"), primary: true }, { label: "Retry", run: onRetry });
      break;
    case "needsSignIn":
      title = agent.loginTitle;
      command = status.command;
      detail = command ? "Run this in a terminal and follow the steps, then try again." : "Sign in with your agent, then try again.";
      // An agent that signs in through the protocol (a browser opens) gets a button for each way it offers
      for (const method of status.methods.filter((method) => !command && method.id === "chat-gpt")) {
        title = agent.loginTitle;
        detail = "A browser opens to sign in with your ChatGPT account.";
        actions.push({ label: signingIn ? "Waiting for the browser…" : `Sign in with ${method.name}`, run: () => signIn(method.id), primary: true });
      }
      if (actions.length === 0) actions.push({ label: "Retry", run: onRetry, primary: true });
      break;
    case "unsupported":
      title = status.message;
      break;
    case "failed":
      title = `Couldn't start ${agent.kind === "custom" ? "the agent" : agent.name}: ${status.message}`;
      actions.push({ label: "Retry", run: onRetry, primary: true }, { label: "Show log", run: showLog });
      break;
    case "stopped":
      title = "The assistant stopped";
      actions.push({ label: "Restart", run: onRetry, primary: true }, { label: "Show log", run: showLog });
      break;
  }

  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <span className="flex size-11 items-center justify-center rounded-2xl bg-warning/14 text-warning-foreground">
        <TriangleAlert size={20} />
      </span>
      <p className="mt-4 max-w-xs text-sm font-medium">{title}</p>
      {detail && <p className="mt-1 max-w-xs text-[13px] leading-relaxed text-muted-foreground">{detail}</p>}
      {command && (
        <>
          <div className="mt-3 flex max-w-xs items-start gap-1 rounded-lg bg-muted py-1.5 pl-3 pr-1.5 text-left">
            <code className="min-w-0 flex-1 select-text break-all font-mono text-xs leading-relaxed">{command}</code>
            <Button
              variant="ghost-muted"
              size="icon-xs"
              aria-label="Copy the command"
              onClick={async () => {
                await writeClipboardText(command);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
            >
              {copied ? <Check /> : <Copy />}
            </Button>
          </div>
          {agent.kind === "claude" && <p className="mt-2 max-w-xs text-xs leading-relaxed text-muted-foreground">Already use Claude Code? Run claude and use /login instead.</p>}
        </>
      )}
      {signInError && <p className="mt-3 max-w-xs text-xs leading-relaxed text-destructive-foreground">{signInError}</p>}
      {actions.length > 0 && (
        <div className="mt-5 flex gap-2">
          {actions.map((action) => (
            <Button key={action.label} variant={action.primary ? "primary" : "outline"} size="sm" disabled={signingIn} onClick={action.run}>
              {action.label}
            </Button>
          ))}
        </div>
      )}
      <Dialog open={log !== null} onClose={() => setLog(null)} title="Assistant log" className="max-w-2xl">
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed whitespace-pre-wrap">
          {log && log.length > 0 ? log.join("\n") : "Nothing was logged."}
        </pre>
      </Dialog>
    </div>
  );
}
