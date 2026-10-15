import { useCallback, useMemo, useState, type ReactNode } from "react";
import { api } from "../lib/api";
import { disposeTabState } from "../lib/tabState";
import { WorkspaceContext, type TabSessionInfo, type WorkspaceTab } from "./workspace";

function newTab(): WorkspaceTab {
  return { id: crypto.randomUUID(), title: null, subtitle: null, color: null, sessionId: null };
}

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [tabs, setTabs] = useState<WorkspaceTab[]>(() => [newTab()]);
  const [activeTabId, setActiveTabId] = useState(() => tabs[0].id);

  const openTab = useCallback(() => {
    const tab = newTab();
    setTabs((prev) => [...prev, tab]);
    setActiveTabId(tab.id);
  }, []);

  const closeTab = useCallback(
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
    () => ({ tabs, activeTabId, openTab, closeTab, activateTab: setActiveTabId, cycleTab, updateTab }),
    [tabs, activeTabId, openTab, closeTab, cycleTab, updateTab],
  );

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}
