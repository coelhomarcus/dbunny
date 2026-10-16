import { useCallback, useMemo, useState, type ReactNode } from "react";
import { GitCommitVertical } from "lucide-react";
import { api, createSessionApi } from "../lib/api";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import { disposeTabState } from "../lib/tabState";
import { WorkspaceContext, type TabSessionInfo, type WorkspaceTab } from "./workspace";
import type { InitialSession } from "@/types";

const SIDEBAR_KEY = "dbunny-sidebar-open";

function readSidebarOpen() {
  try {
    return localStorage.getItem(SIDEBAR_KEY) !== "false";
  } catch {
    return true;
  }
}

function newTab(initialSession?: InitialSession): WorkspaceTab {
  return { id: crypto.randomUUID(), title: null, subtitle: null, color: null, icon: null, sessionId: null, initialSession };
}

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [tabs, setTabs] = useState<WorkspaceTab[]>(() => [newTab()]);
  const [activeTabId, setActiveTabId] = useState(() => tabs[0].id);
  const [sidebarOpen, setSidebarOpenState] = useState(readSidebarOpen);

  const setSidebarOpen = useCallback((open: boolean) => {
    setSidebarOpenState(open);
    try {
      localStorage.setItem(SIDEBAR_KEY, String(open));
    } catch {
      // not persisted, still applied
    }
  }, []);

  const toggleSidebar = useCallback(() => setSidebarOpen(!sidebarOpen), [sidebarOpen, setSidebarOpen]);

  const openTab = useCallback((initialSession?: InitialSession) => {
    const tab = newTab(initialSession);
    setTabs((prev) => [...prev, tab]);
    setActiveTabId(tab.id);
  }, []);

  const closeNow = useCallback(
    (id: string) => {
      const index = tabs.findIndex((t) => t.id === id);
      if (index < 0) return;
      const closing = tabs[index];
      if (closing.sessionId) api.disconnect(closing.sessionId).catch(() => {});
      disposeTabState(id);

      // Always keep one tab open
      const remaining = tabs.length === 1 ? [newTab()] : tabs.filter((t) => t.id !== id);
      setTabs(remaining);
      if (id === activeTabId || tabs.length === 1) {
        setActiveTabId(remaining[Math.min(index, remaining.length - 1)].id);
      }
    },
    [tabs, activeTabId],
  );

  // A tab whose open transaction would be rolled back by closing it, waiting for a yes
  const [confirmClose, setConfirmClose] = useState<string | null>(null);

  /** Closes a tab, asking first when closing would roll back a transaction opened in it. */
  const closeTab = useCallback(
    async (id: string) => {
      const sessionId = tabs.find((t) => t.id === id)?.sessionId;
      const inTransaction = sessionId ? await createSessionApi(sessionId).transactionOpen().catch(() => false) : false;
      if (inTransaction) setConfirmClose(id);
      else closeNow(id);
    },
    [tabs, closeNow],
  );

  const cycleTab = useCallback(
    (direction: 1 | -1) => {
      const index = tabs.findIndex((t) => t.id === activeTabId);
      setActiveTabId(tabs[(index + direction + tabs.length) % tabs.length].id);
    },
    [tabs, activeTabId],
  );

  const updateTab = useCallback((id: string, info: TabSessionInfo) => {
    setTabs((prev) => prev.map((t) => (t.id === id ? { ...t, ...info } : t)));
  }, []);

  const value = useMemo(
    () => ({
      tabs,
      activeTabId,
      openTab,
      closeTab,
      activateTab: setActiveTabId,
      cycleTab,
      updateTab,
      sidebarOpen,
      setSidebarOpen,
      toggleSidebar,
    }),
    [tabs, activeTabId, openTab, closeTab, cycleTab, updateTab, sidebarOpen, setSidebarOpen, toggleSidebar],
  );

  return (
    <WorkspaceContext.Provider value={value}>
      {children}
      <ConfirmDialog
        open={confirmClose !== null}
        onClose={() => setConfirmClose(null)}
        onConfirm={() => {
          if (confirmClose) closeNow(confirmClose);
          setConfirmClose(null);
        }}
        icon={<GitCommitVertical />}
        tone="destructive"
        title="Close with a transaction open?"
        description="Its changes haven't been committed, so closing the tab rolls them back."
        confirmLabel="Roll back and close"
      />
    </WorkspaceContext.Provider>
  );
}
