import { useState } from "react";
import { Loader2, TriangleAlert } from "lucide-react";
import { assistantApi } from "../../lib/api";
import { openExternal } from "../../lib/openExternal";
import type { AgentStatus } from "../../types/assistant";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";

/** The assistant's agent is not ready: what is missing, and what to do about it. */
export default function AssistantStatusScreen({ status, onRetry }: { status: AgentStatus; onRetry: () => void }) {
  const [log, setLog] = useState<string[] | null>(null);

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
  const actions: { label: string; run: () => void; primary?: boolean }[] = [];
  switch (status.state) {
    case "needsNode":
      title = "Node.js 20 or newer is needed";
      detail = status.found ? `Found ${status.found}` : null;
      actions.push({ label: "Get Node.js", run: () => openExternal("https://nodejs.org"), primary: true }, { label: "Retry", run: onRetry });
      break;
    case "needsSignIn":
      title = "Sign in to Claude";
      detail = "Run claude in a terminal, use /login, then try again.";
      actions.push({ label: "Retry", run: onRetry, primary: true });
      break;
    case "unsupported":
      title = status.message;
      break;
    case "failed":
      title = `Couldn't start the Claude agent: ${status.message}`;
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
      {actions.length > 0 && (
        <div className="mt-5 flex gap-2">
          {actions.map((action) => (
            <Button key={action.label} variant={action.primary ? "primary" : "outline"} size="sm" onClick={action.run}>
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
