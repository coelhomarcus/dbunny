import { ShieldCheck } from "lucide-react";
import type { AgentInfo } from "../../lib/assistantAgents";
import Button from "../ui/Button";
import { useT } from "../../i18n";

/** What the assistant sends and keeps, before anything is started. */
export default function ConsentScreen({ agent, onEnable }: { agent: AgentInfo; onEnable: () => void }) {
  const t = useT();
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <span className="flex size-11 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
        <ShieldCheck size={20} />
      </span>
      <h2 className="mt-4 text-sm font-semibold">{t("assistant.consentScreen.askYourDatabase")}</h2>
      <p className="mt-2 max-w-xs text-[13px] leading-relaxed text-muted-foreground">
        {agent.kind === "custom"
          ? t("assistant.consentScreen.custom")
          : t(agent.kind === "codex" ? "assistant.consentScreen.codex" : "assistant.consentScreen.claude", { name: agent.name, account: agent.account, company: agent.company, folder: agent.folder })}
      </p>
      <Button variant="primary" size="sm" className="mt-5" onClick={onEnable}>
        {t("assistant.consentScreen.enableAssistant")}
      </Button>
    </div>
  );
}
