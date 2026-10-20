import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import Spinner from "../ui/Spinner";
import type { AgentInfo } from "../../lib/assistantAgents";
import { startupHint, startupRows, type StartStepId } from "../../lib/assistantStartup";
import { cn } from "../../lib/cn";
import AgentMark from "./AgentMark";
import { useT } from "../../i18n";

/** The assistant is starting: each thing DBunny does, where it is now, and what the agent says while it works. */
export default function StartingScreen({ agent, step, detail }: { agent: AgentInfo; step: StartStepId; detail: string | null }) {
  const t = useT();
  // Seconds in the step, to say more when one takes long
  const [elapsed, setElapsed] = useState({ step, seconds: 0 });
  useEffect(() => {
    const started = Date.now();
    const timer = setInterval(() => setElapsed({ step, seconds: Math.floor((Date.now() - started) / 1000) }), 1000);
    return () => clearInterval(timer);
  }, [step]);
  const seconds = elapsed.step === step ? elapsed.seconds : 0;
  const hint = startupHint(step, seconds, agent.kind);

  return (
    <div className="flex h-full flex-col items-center justify-center px-6">
      <AgentMark agent={agent} />
      <p className="mt-4 text-sm font-medium">{t("assistant.startingScreen.startingTheAssistant")}</p>
      <ul className="mt-4 w-full max-w-xs space-y-2">
        {startupRows(step, agent.name).map((row) => (
          <li key={row.id} className={cn("flex items-start gap-2.5 text-[13px]", row.state === "pending" && "text-muted-foreground/60", row.state === "done" && "text-muted-foreground")}>
            <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center">
              {row.state === "done" ? <Check size={14} className="text-success-foreground" /> : row.state === "current" ? <Spinner size={14} /> : <span className="size-1.5 rounded-full bg-current" />}
            </span>
            <span className="min-w-0 flex-1">
              <span className={cn(row.state === "current" && "font-medium text-foreground")}>{row.label}</span>
              {row.state === "current" && detail && <code className="mt-1 block truncate font-mono text-[11px] text-muted-foreground">{detail}</code>}
              {row.state === "current" && hint && <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">{hint}</span>}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
