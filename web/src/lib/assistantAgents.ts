export type AgentKind = "claude" | "codex" | "custom";

/** What the interface says about an agent. */
export interface AgentInfo {
  kind: AgentKind;
  name: string;
  /** The account it uses. */
  account: string;
  /** Who receives the schema and the results the person allows. */
  company: string;
  /** Where it keeps its conversations. */
  folder: string;
  loginTitle: string;
}

const AGENTS: Record<AgentKind, AgentInfo> = {
  claude: { kind: "claude", name: "Claude Code", account: "Claude", company: "Anthropic", folder: "~/.claude/projects", loginTitle: "Sign in to Claude" },
  codex: { kind: "codex", name: "Codex", account: "ChatGPT", company: "OpenAI", folder: "~/.codex", loginTitle: "Sign in to Codex" },
  custom: {
    kind: "custom",
    name: "your agent",
    account: "its",
    company: "the agent's provider",
    folder: "the agent's own folder",
    loginTitle: "Sign in to your agent",
  },
};

export function agentInfo(kind: AgentKind): AgentInfo {
  return AGENTS[kind];
}

function isAgent(value: unknown): value is AgentKind {
  return value === "claude" || value === "codex" || value === "custom";
}

/** The agent the settings stored; before there was a choice, a command of the person's own meant a custom agent. */
export function agentFromStored(stored: unknown): AgentKind {
  const settings = stored !== null && typeof stored === "object" ? (stored as Record<string, unknown>) : {};
  if (isAgent(settings.assistantAgent)) return settings.assistantAgent;
  return typeof settings.assistantCommand === "string" && settings.assistantCommand.trim() !== "" ? "custom" : "claude";
}
