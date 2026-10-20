import { useState } from "react";
import { Check, Copy, TriangleAlert } from "lucide-react";
import Spinner from "../ui/Spinner";
import { assistantApi } from "../../lib/api";
import { writeClipboardText } from "../../lib/clipboard";
import { openExternal } from "../../lib/openExternal";
import { signInMethods, type AgentInfo } from "../../lib/assistantAgents";
import type { AgentStatus } from "../../types/assistant";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import AgentMark from "./AgentMark";
import StartingScreen from "./StartingScreen";
import { useT } from "../../i18n";

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
  const t = useT();
  const [log, setLog] = useState<string[] | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [signInError, setSignInError] = useState<string | null>(null);

  async function signIn(methodId: string) {
    setSigningIn(true);
    setSignInError(null);
    try {
      await onSignIn(methodId);
    } catch (error) {
      setSignInError(typeof error === "string" ? error : error instanceof Error ? error.message : t("assistant.assistantStatusScreen.loginFailed"));
    } finally {
      setSigningIn(false);
    }
  }
  const [copied, setCopied] = useState(false);

  async function showLog() {
    setLog(await assistantApi.log().catch(() => []));
  }

  if (status.state === "starting") return <StartingScreen agent={agent} step={status.step} detail={status.detail} />;
  if (status.state === "idle" || status.state === "ready") {
    return (
      <div className="flex h-full flex-col items-center justify-center">
        <AgentMark agent={agent} />
        <p className="mt-4 text-sm font-medium">{t("assistant.assistantStatusScreen.startingTheAssistant")}</p>
        <Spinner size={15} className="mt-3 text-muted-foreground" />
      </div>
    );
  }

  let title: string;
  let detail: string | null = null;
  let command: string | null = null;
  const actions: { label: string; run: () => void; primary?: boolean }[] = [];
  switch (status.state) {
    case "needsNode":
      title = t("assistant.assistantStatusScreen.needsNode");
      detail = status.found ? t("assistant.assistantStatusScreen.found", { version: status.found }) : null;
      actions.push({ label: t("assistant.assistantStatusScreen.getNodeJs"), run: () => openExternal("https://nodejs.org"), primary: true }, { label: t("common.retry"), run: onRetry });
      break;
    case "needsSignIn":
      title = agent.loginTitle;
      command = status.command;
      detail = command ? t("assistant.assistantStatusScreen.runInTerminal") : t("assistant.assistantStatusScreen.signInThenRetry");
      // An agent that signs in through the protocol (a browser may open) gets a button for each way it offers
      if (!command) {
        const methods = signInMethods(agent.kind, status.methods);
        if (methods.length > 0) detail = agent.kind === "codex" ? t("assistant.assistantStatusScreen.browserOpensChatGpt") : t("assistant.assistantStatusScreen.browserMayOpen");
        methods.forEach((method, index) =>
          actions.push({ label: signingIn ? t("assistant.assistantStatusScreen.waitingForBrowser") : t("assistant.assistantStatusScreen.signInWith", { method: method.name }), run: () => signIn(method.id), primary: index === 0 }),
        );
      }
      if (actions.length === 0) actions.push({ label: t("common.retry"), run: onRetry, primary: true });
      break;
    case "unsupported":
      title = status.message;
      break;
    case "failed":
      title = t("assistant.assistantStatusScreen.couldntStart", { agent: agent.kind === "custom" ? t("assistant.assistantStatusScreen.theAgent") : agent.name, message: status.message });
      actions.push({ label: t("common.retry"), run: onRetry, primary: true }, { label: t("assistant.assistantStatusScreen.showLog"), run: showLog });
      break;
    case "stopped":
      title = t("assistant.assistantStatusScreen.stopped");
      actions.push({ label: t("assistant.assistantStatusScreen.restart"), run: onRetry, primary: true }, { label: t("assistant.assistantStatusScreen.showLog"), run: showLog });
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
              aria-label={t("assistant.assistantStatusScreen.copyTheCommand")}
              onClick={async () => {
                await writeClipboardText(command);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
            >
              {copied ? <Check /> : <Copy />}
            </Button>
          </div>
          {agent.kind === "claude" && <p className="mt-2 max-w-xs text-xs leading-relaxed text-muted-foreground">{t("assistant.assistantStatusScreen.alreadyUseClaudeCodeRun")}</p>}
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
      <Dialog open={log !== null} onClose={() => setLog(null)} title={t("assistant.assistantStatusScreen.assistantLog")} className="max-w-2xl">
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed whitespace-pre-wrap">
          {log && log.length > 0 ? log.join("\n") : t("assistant.assistantStatusScreen.nothingLogged")}
        </pre>
      </Dialog>
    </div>
  );
}
