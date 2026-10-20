import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { AssistantContext } from "./assistant";
import { useConnection } from "./useConnection";
import { useTab } from "./workspace";
import { useDocuments } from "./DocumentsContext";
import { useCapabilities } from "../hooks/useCapabilities";
import { assistantApi } from "../lib/api";
import {
  addUserMessage,
  applyUpdate,
  attachApproval,
  emptyConversation,
  finishTurn,
  resolveApproval,
  type Conversation,
} from "../lib/assistantConversation";
import { agentInfo } from "../lib/assistantAgents";
import { ConversationOpener } from "../lib/assistantOpener";
import { conversationOptions, getConnectionPrefs } from "../lib/assistantPrefs";
import { isAssistantShortcut } from "../lib/assistantShortcut";
import { isMac } from "../lib/platform";
import { updateSettings, useSettings } from "../lib/settings";
import type { AgentStatus, ApprovalRequest, Decision, ModelChoice, OpenedConversation, SessionSummary } from "../types/assistant";
import { t, useLocale } from "../i18n";
import { errorText } from "../lib/appError";

function messageOf(error: unknown): string {
  return errorText(error, t("ui.assistantProvider.wentWrong"));
}

/** The assistant of one connection tab: its agent's status, and the conversation held in it. */
export function AssistantProvider({ children }: { children: ReactNode }) {
  const capabilities = useCapabilities();
  const { sessionId, savedConnectionId } = useConnection();
  const { isActive } = useTab();
  const [settings] = useSettings();
  const { documents, openDocument, closeDocument } = useDocuments();
  const assistantTab = documents.items.find((doc) => doc.kind === "assistant");
  const inTab = assistantTab !== undefined && assistantTab.id === documents.activeId;

  const [status, setStatus] = useState<AgentStatus>({ state: "idle" });
  const [panelOpen, setPanelOpen] = useState(false);
  const [conversation, setConversation] = useState<Conversation>(emptyConversation);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelChoice | null>(null);
  const [history, setHistory] = useState<SessionSummary[]>([]);
  const [queued, setQueued] = useState<string | null>(null);
  // A past conversation being opened: shown at once, so that the wait is seen as progress
  const [openingTitle, setOpeningTitle] = useState<string | null>(null);
  const openToken = useRef(0);

  // The id the event handlers compare with, and the opener that keeps the updates that arrive while
  // a conversation is opening (a reopened one replays before its id is known)
  const idRef = useRef<string | null>(null);
  const opener = useRef<ConversationOpener | null>(null);
  if (opener.current === null) opener.current = new ConversationOpener((id) => void assistantApi.closeConversation(id).catch(() => {}));
  // The conversation made ahead of the first message, so that the model can be picked before it
  const preparing = useRef<Promise<void> | null>(null);
  const createdWith = useRef("");
  const emptyRef = useRef(true);

  const available = capabilities.assistant;
  // The agent chosen in Settings; the person's own command is for a custom one only
  const agent = settings.assistantAgent;
  const command = agent === "custom" ? settings.assistantCommand || undefined : undefined;
  const enabled = settings.assistantEnabled;

  // The agent's status and the conversation's updates come as events
  useEffect(() => {
    const stops = [
      listen<AgentStatus>("assistant:status", (event) => setStatus(event.payload)),
      listen<{ conversationId: string; update: unknown }>("assistant:update", (event) => {
        const { conversationId: id, update } = event.payload;
        if (idRef.current === id) {
          setConversation((current) => applyUpdate(current, update, Date.now()));
        } else {
          opener.current?.receive(id, update);
        }
      }),
      listen<ApprovalRequest>("assistant:approval", (event) => {
        if (idRef.current === event.payload.conversationId) setConversation((current) => attachApproval(current, event.payload));
      }),
    ];
    assistantApi.status().then(setStatus).catch(() => {});
    return () => {
      stops.forEach((stop) => stop.then((fn) => fn()));
    };
  }, []);

  const locale = useLocale();
  const options = useCallback(() => conversationOptions(settings, getConnectionPrefs(savedConnectionId), locale), [settings, savedConnectionId, locale]);

  const forgetConversation = useCallback(() => {
    const id = idRef.current;
    idRef.current = null;
    // Whatever is opening is no longer wanted either
    opener.current?.invalidate();
    openToken.current++;
    setOpeningTitle(null);
    preparing.current = null;
    setConversationId(null);
    setConversation(emptyConversation());
    setModels(null);
    if (id) assistantApi.closeConversation(id).catch(() => {});
  }, []);

  /** Opens a conversation and shows what the agent replays of it; null when another opening came first. */
  const begin = useCallback(async (open: () => Promise<OpenedConversation>) => {
    const result = await opener.current!.open(open);
    if (!result) return null;
    idRef.current = result.opened.conversationId;
    setConversationId(result.opened.conversationId);
    setModels(result.opened.models);
    setConversation(result.state);
    return result.opened;
  }, []);

  /** Makes sure there is a conversation to write to: one made with what the settings say now. */
  const ensureConversation = useCallback(async () => {
    if (!sessionId) return;
    const key = JSON.stringify(options());
    // A conversation that is still empty is made again when the options changed meanwhile
    if (idRef.current && emptyRef.current && key !== createdWith.current) forgetConversation();
    if (idRef.current) return;
    if (!preparing.current) {
      const making: Promise<void> = begin(() => assistantApi.newConversation(sessionId, savedConnectionId, options()))
        .then((opened) => {
          if (opened) createdWith.current = key;
        })
        .finally(() => {
          if (preparing.current === making) preparing.current = null;
        });
      preparing.current = making;
    }
    await preparing.current;
    // Another conversation (a past one) was being opened over this one
    if (!idRef.current) throw new Error("The conversation is still opening. Try again in a moment.");
  }, [sessionId, savedConnectionId, options, forgetConversation, begin]);

  const refreshHistory = useCallback(async () => {
    if (!sessionId || !savedConnectionId) {
      setHistory([]);
      return;
    }
    try {
      setHistory(await assistantApi.listConversations(sessionId, savedConnectionId));
    } catch {
      setHistory([]);
    }
  }, [sessionId, savedConnectionId]);

  const send = useCallback(
    async (text: string) => {
      if (!sessionId || !text.trim()) return;
      try {
        await ensureConversation();
      } catch (error) {
        setConversation((current) => finishTurn(addUserMessage(current, text), messageOf(error)));
        return;
      }
      const id = idRef.current;
      if (!id) return;
      setConversation((current) => addUserMessage(current, text));
      // The turn ends in the conversation it was sent in, which may have been left meanwhile
      try {
        const reason = await assistantApi.prompt(id, text);
        if (idRef.current === id) setConversation((current) => finishTurn(current, undefined, reason === "cancelled" ? "Stopped" : undefined));
      } catch (error) {
        if (idRef.current === id) setConversation((current) => finishTurn(current, messageOf(error)));
      }
      refreshHistory();
    },
    [sessionId, ensureConversation, refreshHistory],
  );

  const stop = useCallback(async () => {
    if (idRef.current) await assistantApi.cancel(idRef.current).catch(() => {});
  }, []);

  const approve = useCallback(async (approvalId: string, decision: Decision) => {
    setConversation((current) => resolveApproval(current, approvalId));
    await assistantApi.approve(approvalId, decision).catch(() => {});
  }, []);

  const newChat = useCallback(async () => {
    forgetConversation();
  }, [forgetConversation]);

  const openConversation = useCallback(
    async (acpSession: string, title?: string) => {
      if (!sessionId) return;
      forgetConversation();
      // The conversation is on screen as opening from the click, not from when the agent answers
      const token = openToken.current;
      setOpeningTitle(title ?? "");
      try {
        await begin(() => assistantApi.loadConversation(sessionId, savedConnectionId, acpSession, options()));
      } catch (error) {
        setConversation((current) => finishTurn(current, messageOf(error)));
        refreshHistory();
      } finally {
        if (openToken.current === token) setOpeningTitle(null);
      }
    },
    [sessionId, savedConnectionId, forgetConversation, begin, options, refreshHistory],
  );

  const setModel = useCallback(async (modelId: string) => {
    if (!idRef.current) return;
    await assistantApi.setModel(idRef.current, modelId);
    setModels((current) => (current ? { ...current, current: modelId } : current));
  }, []);

  const retry = useCallback(async () => {
    setStatus(status.state === "idle" ? await assistantApi.start(agent, command) : await assistantApi.restart(agent, command));
  }, [agent, command, status.state]);

  const signIn = useCallback(async (methodId: string) => {
    // The status comes back as an event, ready, once the agent is signed in
    await assistantApi.authenticate(methodId);
  }, []);

  // The agent went away (stopped, or signed out): what was said to it can't go on
  const lastState = useRef(status.state);
  useEffect(() => {
    if (lastState.current === "ready" && status.state !== "ready") forgetConversation();
    lastState.current = status.state;
  }, [status.state, forgetConversation]);

  const ask = useCallback((text: string) => {
    // Written to in the tab if that is in front, in the panel otherwise
    if (!inTab) setPanelOpen(true);
    setQueued(text);
  }, [inTab]);

  const expand = useCallback(() => openDocument({ kind: "assistant" }), [openDocument]);
  const backToPanel = useCallback(() => {
    if (assistantTab) closeDocument(assistantTab.id);
    setPanelOpen(true);
  }, [assistantTab, closeDocument]);

  // The agent starts the first time the assistant is looked at
  const wanted = available && enabled && (panelOpen || inTab);
  useEffect(() => {
    if (wanted && status.state === "idle") assistantApi.start(agent, command).then(setStatus).catch((e) => setStatus({ state: "failed", message: messageOf(e) }));
  }, [wanted, status.state, agent, command]);

  // Another agent was chosen: the one that is up gives way to it
  const lastAgent = useRef(agent);
  const stateRef = useRef(status.state);
  useEffect(() => {
    stateRef.current = status.state;
  }, [status.state]);
  useEffect(() => {
    if (lastAgent.current === agent) return;
    lastAgent.current = agent;
    if (stateRef.current !== "idle") assistantApi.restart(agent, command).then(setStatus).catch((e) => setStatus({ state: "failed", message: messageOf(e) }));
  }, [agent, command]);

  // A question asked from the editor goes once the assistant can take it
  useEffect(() => {
    if (queued && enabled && status.state === "ready" && sessionId) {
      setQueued(null);
      send(queued);
    }
  }, [queued, enabled, status.state, sessionId, send]);

  useEffect(() => {
    if (wanted && status.state === "ready") refreshHistory();
  }, [wanted, status.state, refreshHistory]);

  useEffect(() => {
    emptyRef.current = conversation.items.length === 0;
  }, [conversation]);

  // With the agent ready, a conversation is made at once (a new one after "New chat"), which is when the models are known
  useEffect(() => {
    if (wanted && status.state === "ready" && sessionId && !conversationId && !opener.current?.opening) ensureConversation().catch(() => {});
  }, [wanted, status.state, sessionId, conversationId, ensureConversation]);

  // The conversation belongs to the session: another session, or none, starts afresh
  useEffect(() => {
    return () => {
      const id = idRef.current;
      idRef.current = null;
      if (id) assistantApi.closeConversation(id).catch(() => {});
    };
  }, [sessionId]);
  useEffect(() => {
    setConversation(emptyConversation());
    setConversationId(null);
    setModels(null);
    setHistory([]);
  }, [sessionId]);

  // Cmd+I (Ctrl+I) opens and closes the panel of the tab in front
  useEffect(() => {
    if (!available || !isActive) return;
    function onKeyDown(event: KeyboardEvent) {
      if (isAssistantShortcut(event, isMac)) {
        event.preventDefault();
        // Before the SQL editor sees it: its own Cmd+I selects the parent syntax node
        event.stopPropagation();
        setPanelOpen((open) => !open);
      }
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [available, isActive]);

  const value = useMemo(
    () => ({
      available,
      status,
      enabled,
      enable: () => updateSettings({ assistantEnabled: true }),
      panelOpen,
      setPanelOpen,
      toggle: () => setPanelOpen((open) => !open),
      inTab,
      expand,
      backToPanel,
      conversation,
      conversationId,
      models,
      history,
      refreshHistory,
      send,
      stop,
      approve,
      newChat,
      openConversation,
      openingTitle,
      setModel,
      ask,
      retry,
      signIn,
      agent: agentInfo(agent),
    }),
    [available, status, enabled, panelOpen, inTab, expand, backToPanel, conversation, conversationId, models, history, refreshHistory, send, stop, approve, newChat, openConversation, openingTitle, setModel, ask, retry, signIn, agent],
  );

  return <AssistantContext.Provider value={value}>{children}</AssistantContext.Provider>;
}
