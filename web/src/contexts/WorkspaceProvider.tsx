import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import { GitCommitVertical } from "lucide-react";
import { api, createSessionApi } from "../lib/api";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import { disposeTabState } from "../lib/tabState";
import { WorkspaceContext, type TabSessionInfo, type WorkspaceTab } from "./workspace";
import type { InitialSession } from "@/types";
import { useT } from "../i18n";

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
  const t = useT();
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

  const snapshot = useRef({ tabs, activeTabId });
  snapshot.current = { tabs, activeTabId };

  const closeNow = useCallback((ids: string[]) => {
    const { tabs: open, activeTabId: active } = snapshot.current;
    const drop = new Set(ids);
    const closing = open.filter((tab) => drop.has(tab.id));
    if (closing.length === 0) return;
    for (const tab of closing) {
      if (tab.sessionId) api.disconnect(tab.sessionId).catch(() => {});
      disposeTabState(tab.id);
    }
    let remaining = open.filter((tab) => !drop.has(tab.id));
    let nextActive = active;
    if (remaining.length === 0) {
      const fresh = newTab();
      remaining = [fresh];
      nextActive = fresh.id;
    } else if (drop.has(active)) {
      const index = open.findIndex((tab) => tab.id === active);
      let neighbor: WorkspaceTab | undefined;
      for (let i = index + 1; i < open.length; i++) {
        if (!drop.has(open[i].id)) {
          neighbor = open[i];
          break;
        }
      }
      if (!neighbor) {
        for (let i = index - 1; i >= 0; i--) {
          if (!drop.has(open[i].id)) {
            neighbor = open[i];
            break;
          }
        }
      }
      nextActive = neighbor?.id ?? remaining[0].id;
    }
    snapshot.current = { tabs: remaining, activeTabId: nextActive };
    setTabs(remaining);
    setActiveTabId(nextActive);
  }, []);

  // Tabs whose open transaction would be rolled back by closing them, waiting for a yes
  const [confirmClose, setConfirmClose] = useState<string[] | null>(null);

  /** Closes tabs, asking once when closing would roll back a transaction opened in one of them. */
  const closeTabs = useCallback(
    async (ids: string[]) => {
      const { tabs: open } = snapshot.current;
      const unique = [...new Set(ids)].filter((id) => open.some((tab) => tab.id === id));
      if (unique.length === 0) return;
      let inTransaction = false;
      for (const id of unique) {
        const sessionId = open.find((tab) => tab.id === id)?.sessionId;
        if (sessionId && (await createSessionApi(sessionId).transactionOpen().catch(() => false))) {
          inTransaction = true;
          break;
        }
      }
      if (inTransaction) setConfirmClose(unique);
      else closeNow(unique);
    },
    [closeNow],
  );

  const closeTab = useCallback((id: string) => closeTabs([id]), [closeTabs]);

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
      closeTabs,
      activateTab: setActiveTabId,
      cycleTab,
      updateTab,
      sidebarOpen,
      setSidebarOpen,
      toggleSidebar,
    }),
    [tabs, activeTabId, openTab, closeTab, closeTabs, cycleTab, updateTab, sidebarOpen, setSidebarOpen, toggleSidebar],
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
        title={t("ui.workspaceProvider.closeWithTransaction")}
        description={
          confirmClose && confirmClose.length > 1
            ? t("ui.workspaceProvider.manyOpenTransactions")
            : t("ui.workspaceProvider.oneOpenTransaction")
        }
        confirmLabel={t("ui.workspaceProvider.rollBackAndClose")}
      />
    </WorkspaceContext.Provider>
  );
}
