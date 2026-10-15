import { createContext, useContext } from "react";

/** One window tab = one independent connection workspace. */
export interface WorkspaceTab {
  id: string;
  /** Saved connection name or database, null while on the connect screen. */
  title: string | null;
  /** host:port/database, shown as a hint. */
  subtitle: string | null;
  /** Colour of the saved connection, if any. */
  color: string | null;
  sessionId: string | null;
}

export type TabSessionInfo = Pick<WorkspaceTab, "title" | "subtitle" | "color" | "sessionId">;

export interface WorkspaceValue {
  tabs: WorkspaceTab[];
  activeTabId: string;
  openTab: () => void;
  closeTab: (id: string) => void;
  activateTab: (id: string) => void;
  /** Activate the next/previous tab (wraps around). */
  cycleTab: (direction: 1 | -1) => void;
  updateTab: (id: string, info: TabSessionInfo) => void;
}

export const WorkspaceContext = createContext<WorkspaceValue | null>(null);

export function useWorkspace() {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) throw new Error("useWorkspace must be used within WorkspaceProvider");
  return ctx;
}

export interface TabValue {
  tabId: string;
  /** False while the tab is hidden behind another one. */
  isActive: boolean;
}

export const TabContext = createContext<TabValue | null>(null);

export function useTab() {
  const ctx = useContext(TabContext);
  if (!ctx) throw new Error("useTab must be used within a workspace tab");
  return ctx;
}
