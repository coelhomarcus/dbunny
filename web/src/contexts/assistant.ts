import { createContext, useContext } from "react";
import type { Conversation } from "../lib/assistantConversation";
import type { AgentStatus, Decision, ModelChoice, SessionSummary } from "../types/assistant";

export interface AssistantValue {
  /** The connected database can be read by the assistant. */
  available: boolean;
  status: AgentStatus;
  /** The person read what the assistant sends and turned it on. */
  enabled: boolean;
  enable: () => void;
  panelOpen: boolean;
  setPanelOpen: (open: boolean) => void;
  toggle: () => void;
  /** The conversation is in front in a tab of its own, not in the side panel. */
  inTab: boolean;
  /** Moves the conversation to a tab of its own. */
  expand: () => void;
  /** Closes that tab and shows the conversation in the side panel again. */
  backToPanel: () => void;
  conversation: Conversation;
  conversationId: string | null;
  models: ModelChoice | null;
  /** Past conversations of this saved connection and database. */
  history: SessionSummary[];
  refreshHistory: () => Promise<void>;
  send: (text: string) => Promise<void>;
  stop: () => Promise<void>;
  approve: (approvalId: string, decision: Decision) => Promise<void>;
  newChat: () => Promise<void>;
  openConversation: (acpSession: string) => Promise<void>;
  setModel: (modelId: string) => Promise<void>;
  /** Opens the assistant and sends `text` as soon as it can. */
  ask: (text: string) => void;
  /** Starts the agent again after it stopped or could not start. */
  retry: () => Promise<void>;
}

export const AssistantContext = createContext<AssistantValue | null>(null);

export function useAssistant() {
  const value = useContext(AssistantContext);
  if (!value) throw new Error("useAssistant must be used within AssistantProvider");
  return value;
}
