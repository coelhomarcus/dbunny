import { createContext, useContext } from "react";
import type { InitialSession } from "@/types";

/** One window tab = one independent connection workspace. */
export interface WorkspaceTab {
  id: string;
  /** Saved connection name or database, null while on the connect screen. */
  title: string | null;
  /** host:port/database, shown as a hint. */
  subtitle: string | null;
  /** Colour of the saved connection, if any. */
  color: string | null;
  /** Icon of the saved connection, if any. */
  icon: string | null;
  sessionId: string | null;
  /** A session this tab opens with, instead of starting on the connect screen. */
  initialSession?: InitialSession;
}

export type TabSessionInfo = Pick<WorkspaceTab, "title" | "subtitle" | "color" | "icon" | "sessionId">;

export interface WorkspaceValue {
  tabs: WorkspaceTab[];
  activeTabId: string;
  openTab: (initialSession?: InitialSession) => void;
  closeTab: (id: string) => void;
  activateTab: (id: string) => void;
  /** Activate the next/previous tab (wraps around). */
  cycleTab: (direction: 1 | -1) => void;
  updateTab: (id: string, info: TabSessionInfo) => void;
  /** Sidebar of every tab, shared like in a single window: open or collapsed to an icon rail. */
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  toggleSidebar: () => void;
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
