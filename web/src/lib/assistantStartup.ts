import type { AgentKind } from "./assistantAgents";
import { t } from "../i18n/store.ts";

export type StartStepId = "shell" | "node" | "tools" | "agent";

export interface StartupRow {
  id: StartStepId;
  label: string;
  state: "done" | "current" | "pending";
}

const ORDER: StartStepId[] = ["shell", "node", "tools", "agent"];

/** What the assistant does while it starts, in order, with where it is now. `agentName` is how the agent is called. */
export function startupRows(current: StartStepId, agentName: string): StartupRow[] {
  const labels: Record<StartStepId, string> = {
    shell: t("assistant.startup.shell"),
    node: t("assistant.startup.node"),
    tools: t("assistant.startup.tools"),
    agent: t("assistant.startup.agent", { agent: agentName }),
  };
  const at = ORDER.indexOf(current);
  return ORDER.map((id, index) => ({ id, label: labels[id], state: index < at ? "done" : index === at ? "current" : "pending" }));
}

/** Something to say when a step takes longer than it should, so that a wait doesn't look like a hang. */
export function startupHint(step: StartStepId, seconds: number, agent: AgentKind): string | null {
  if (step === "shell" && seconds >= 6) return t("assistant.startup.shellSlow");
  if (step === "agent" && agent !== "custom") {
    if (seconds >= 90) return t("assistant.startup.downloadSlow");
    if (seconds >= 8) return t("assistant.startup.firstDownload");
  }
  return null;
}
