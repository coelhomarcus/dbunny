import { t } from "../i18n/store.ts";

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
  /** What signing out reaches, for the sentence that says so. */
  signOutScope: string;
}

/** What the interface says about an agent, in the language of the app. */
export function agentInfo(kind: AgentKind): AgentInfo {
  switch (kind) {
    case "claude":
      return { kind, name: "Claude Code", account: "Claude", company: "Anthropic", folder: "~/.claude/projects", loginTitle: t("assistant.agents.claudeLoginTitle"), signOutScope: t("assistant.agents.claudeSignOutScope") };
    case "codex":
      return { kind, name: "Codex", account: "ChatGPT", company: "OpenAI", folder: t("assistant.agents.codexFolder"), loginTitle: t("assistant.agents.codexLoginTitle"), signOutScope: t("assistant.agents.codexSignOutScope") };
    case "custom":
      return {
        kind,
        name: t("assistant.agents.customName"),
        account: t("assistant.agents.customAccount"),
        company: t("assistant.agents.customCompany"),
        folder: t("assistant.agents.customFolder"),
        loginTitle: t("assistant.agents.customLoginTitle"),
        signOutScope: t("assistant.agents.customSignOutScope"),
      };
  }
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

/** What the agent uses and where things go, in two plain sentences for Settings. */
export function agentSummary(info: AgentInfo): string {
  if (info.kind === "custom") {
    return t("assistant.agents.summaryCustom");
  }
  return t("assistant.agents.summary", { name: info.name, account: info.account, company: info.company, folder: info.folder });
}

/** The ways of signing in that get a button, for an agent that signs in through the protocol (a browser opens). */
export function signInMethods(kind: AgentKind, methods: { id: string; name: string }[]): { id: string; name: string }[] {
  if (kind === "claude") return [];
  // An API key is not something a button can give
  if (kind === "codex") return methods.filter((method) => method.id === "chat-gpt");
  return methods;
}
