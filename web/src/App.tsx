import { useCallback, useEffect, useMemo } from "react";
import { MemoryRouter, Routes, Route, Navigate } from "react-router";
import { ConnectionProvider } from "./contexts/ConnectionContext";
import { WorkspaceProvider } from "./contexts/WorkspaceProvider";
import { TabContext, useWorkspace, type TabSessionInfo } from "./contexts/workspace";
import type { InitialSession } from "@/types";
import { cn } from "./lib/cn";
import TitleBar from "./components/TitleBar";
import ConnectPage from "./pages/ConnectPage";
import DatabaseLayout from "./pages/DatabaseLayout";
import QueryEditor from "./pages/QueryEditor";
import TableView from "./pages/TableView";
import TableStructure from "./pages/TableStructure";
import FunctionView from "./pages/FunctionView";
import ObjectView from "./pages/ObjectView";
import ActivityView from "./pages/ActivityView";
import RolesView from "./pages/RolesView";

export default function App() {
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    async function handleKey(e: KeyboardEvent) {
      if (e.ctrlKey && e.shiftKey && e.key === "I") {
        const { invoke } = await import("@tauri-apps/api/core");
        invoke("open_devtools");
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, []);

  return (
    <WorkspaceProvider>
      <div className="h-screen flex flex-col bg-chrome text-foreground">
        <TitleBar />
        {/* -1px: the page card's top border sits under the title bar's last row, where
            only the active tab (painted above it) hides it */}
        <div className="relative -mt-px flex-1 min-h-0">
          <TabPanes />
        </div>
      </div>
    </WorkspaceProvider>
  );
}

function TabPanes() {
  const { tabs, activeTabId, updateTab } = useWorkspace();
  return tabs.map((tab) => (
    <TabPane key={tab.id} tabId={tab.id} active={tab.id === activeTabId} updateTab={updateTab} initialSession={tab.initialSession} />
  ));
}

/**
 * One window tab: its own connection and its own navigation. Inactive tabs stay
 * mounted (just hidden) so queries, results and scroll positions survive switching.
 */
function TabPane({
  tabId,
  active,
  updateTab,
  initialSession,
}: {
  tabId: string;
  active: boolean;
  updateTab: (id: string, info: TabSessionInfo) => void;
  initialSession?: InitialSession;
}) {
  const onSessionChange = useCallback((info: TabSessionInfo) => updateTab(tabId, info), [tabId, updateTab]);
  const tab = useMemo(() => ({ tabId, isActive: active }), [tabId, active]);

  return (
    <TabContext.Provider value={tab}>
      {/* The tab's own container: sidebar, page and status bar all live inside it, and
          the active tab above joins its top edge */}
      <div
        className={cn(
          "absolute inset-x-1.5 top-0 bottom-1.5 flex flex-col overflow-hidden rounded-[10px] border border-frame-line bg-card",
          !active && "hidden",
        )}
      >
        <ConnectionProvider onSessionChange={onSessionChange} initialSession={initialSession}>
          {/* A tab opened with a session starts on the query editor */}
          <MemoryRouter initialEntries={[initialSession ? "/db/query" : "/"]}>
            <Routes>
              <Route path="/" element={<ConnectPage />} />
              <Route path="/db" element={<DatabaseLayout />}>
                <Route index element={<Navigate to="query" replace />} />
                <Route path="query" element={<QueryEditor />} />
                <Route path="activity" element={<ActivityView />} />
                <Route path="roles" element={<RolesView />} />
                <Route path=":schema/:table" element={<TableView />} />
                <Route path=":schema/:table/structure" element={<TableStructure />} />
                <Route path=":schema/function/:name" element={<FunctionView />} />
                <Route path=":schema/sequence/:name" element={<ObjectView kind="sequence" />} />
                <Route path=":schema/type/:name" element={<ObjectView kind="type" />} />
              </Route>
            </Routes>
          </MemoryRouter>
        </ConnectionProvider>
      </div>
    </TabContext.Provider>
  );
}
