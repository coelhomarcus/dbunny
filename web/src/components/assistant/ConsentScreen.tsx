import { ShieldCheck } from "lucide-react";
import type { AgentInfo } from "../../lib/assistantAgents";
import Button from "../ui/Button";

/** What the assistant sends and keeps, before anything is started. */
export default function ConsentScreen({ agent, onEnable }: { agent: AgentInfo; onEnable: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <span className="flex size-11 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
        <ShieldCheck size={20} />
      </span>
      <h2 className="mt-4 text-sm font-semibold">Ask your database</h2>
      <p className="mt-2 max-w-xs text-[13px] leading-relaxed text-muted-foreground">
        {agent.kind === "custom"
          ? `The assistant runs the agent you chose on this computer. It can read this connection's schema and run queries that only read, after you allow them. Your schema and the results you allow are sent to the agent's provider, and the agent keeps the conversations where it likes. You need Node.js 20 or newer if the agent needs it.`
          : `The assistant runs ${agent.name} on this computer with your ${agent.account} account. It can read this connection's schema and run queries that only read, after you allow them. Your schema and the results you allow are sent to ${agent.company}. ${agent.name} keeps the conversations in ${agent.folder}. You need Node.js 20 or newer and to be signed in to ${agent.name}.`}
        {agent.kind === "codex" && " Codex also loads the tools and hooks from your own ~/.codex/config.toml."}
      </p>
      <Button variant="primary" size="sm" className="mt-5" onClick={onEnable}>
        Enable assistant
      </Button>
    </div>
  );
}
