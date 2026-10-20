import type { AgentKind } from "./assistantAgents";

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
    shell: "Reading your shell settings to find Node.js",
    node: "Checking Node.js",
    tools: "Starting DBunny's tool server",
    agent: `Starting ${agentName}`,
  };
  const at = ORDER.indexOf(current);
  return ORDER.map((id, index) => ({ id, label: labels[id], state: index < at ? "done" : index === at ? "current" : "pending" }));
}

/** Something to say when a step takes longer than it should, so that a wait doesn't look like a hang. */
export function startupHint(step: StartStepId, seconds: number, agent: AgentKind): string | null {
  if (step === "shell" && seconds >= 6) return "Your shell is slow to start. DBunny waits for it a few seconds more.";
  if (step === "agent" && agent !== "custom") {
    if (seconds >= 90) return "Still working. The download is slow, or the connection is. It keeps going while you wait.";
    if (seconds >= 8) return "The first time, DBunny downloads the agent. This can take a few minutes.";
  }
  return null;
}
