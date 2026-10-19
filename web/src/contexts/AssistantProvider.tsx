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
import { conversationOptions, getConnectionPrefs } from "../lib/assistantPrefs";
import { isMac } from "../lib/platform";
import { updateSettings, useSettings } from "../lib/settings";
import type { AgentStatus, ApprovalRequest, Decision, ModelChoice, OpenedConversation, SessionSummary } from "../types/assistant";

function messageOf(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : "Something went wrong";
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

  // The id the event handlers compare with, and the updates that arrive while a conversation is
  // opening (a reopened one replays before its id is known)
  const idRef = useRef<string | null>(null);
  const opening = useRef(false);
  const buffered = useRef(new Map<string, unknown[]>());
  // The conversation made ahead of the first message, so that the model can be picked before it
  const preparing = useRef<Promise<void> | null>(null);
  const createdWith = useRef("");
  const emptyRef = useRef(true);

  const available = capabilities.assistant;
  const enabled = settings.assistantEnabled;

  // The agent's status and the conversation's updates come as events
  useEffect(() => {
    const stops = [
      listen<AgentStatus>("assistant:status", (event) => setStatus(event.payload)),
      listen<{ conversationId: string; update: unknown }>("assistant:update", (event) => {
        const { conversationId: id, update } = event.payload;
        if (idRef.current === id) {
          setConversation((current) => applyUpdate(current, update));
        } else if (opening.current) {
          buffered.current.set(id, [...(buffered.current.get(id) ?? []), update]);
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

  const options = useCallback(() => conversationOptions(settings, getConnectionPrefs(savedConnectionId)), [settings, savedConnectionId]);

  const forgetConversation = useCallback(() => {
    const id = idRef.current;
    idRef.current = null;
    setConversationId(null);
    setConversation(emptyConversation());
    setModels(null);
    if (id) assistantApi.closeConversation(id).catch(() => {});
  }, []);

  /** Opens a conversation and shows what the agent replays of it. */
  const begin = useCallback(async (open: () => Promise<OpenedConversation>) => {
    opening.current = true;
    buffered.current.clear();
    try {
      const opened = await open();
      let state = emptyConversation();
      for (const update of buffered.current.get(opened.conversationId) ?? []) state = applyUpdate(state, update);
      idRef.current = opened.conversationId;
      setConversationId(opened.conversationId);
      setModels(opened.models);
      setConversation(state);
      return opened;
    } finally {
      opening.current = false;
      buffered.current.clear();
    }
  }, []);

  /** Makes sure there is a conversation to write to: one made with what the settings say now. */
  const ensureConversation = useCallback(async () => {
    if (!sessionId) return;
    const key = JSON.stringify(options());
    // A conversation that is still empty is made again when the options changed meanwhile
    if (idRef.current && emptyRef.current && key !== createdWith.current) forgetConversation();
    if (idRef.current) return;
    if (!preparing.current) {
      preparing.current = begin(() => assistantApi.newConversation(sessionId, savedConnectionId, options()))
        .then(() => {
          createdWith.current = key;
        })
        .finally(() => {
          preparing.current = null;
        });
    }
    await preparing.current;
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
      try {
        await assistantApi.prompt(id, text);
        setConversation((current) => finishTurn(current));
      } catch (error) {
        setConversation((current) => finishTurn(current, messageOf(error)));
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
    async (acpSession: string) => {
      if (!sessionId) return;
      forgetConversation();
      try {
        await begin(() => assistantApi.loadConversation(sessionId, savedConnectionId, acpSession, options()));
      } catch (error) {
        setConversation((current) => finishTurn(current, messageOf(error)));
        refreshHistory();
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
    const command = settings.assistantCommand || undefined;
    setStatus(status.state === "idle" ? await assistantApi.start(command) : await assistantApi.restart(command));
  }, [settings.assistantCommand, status.state]);

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
    if (wanted && status.state === "idle") assistantApi.start(settings.assistantCommand || undefined).then(setStatus).catch((e) => setStatus({ state: "failed", message: messageOf(e) }));
  }, [wanted, status.state, settings.assistantCommand]);

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
    if (wanted && status.state === "ready" && sessionId && !conversationId && !opening.current) ensureConversation().catch(() => {});
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
      const modifier = isMac ? event.metaKey : event.ctrlKey;
      if (modifier && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "i") {
        event.preventDefault();
        setPanelOpen((open) => !open);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
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
      setModel,
      ask,
      retry,
    }),
    [available, status, enabled, panelOpen, inTab, expand, backToPanel, conversation, conversationId, models, history, refreshHistory, send, stop, approve, newChat, openConversation, setModel, ask, retry],
  );

  return <AssistantContext.Provider value={value}>{children}</AssistantContext.Provider>;
}
