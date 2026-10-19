import { useNavigate } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useWorkspace } from "../contexts/workspace";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import StatusBar from "../components/layout/StatusBar";
import DatabaseSwitcher from "../components/layout/DatabaseSwitcher";
import { cn } from "../lib/cn";
import { DocumentsProvider } from "../contexts/DocumentsContext";
import DocumentTabBar from "../components/workspace/DocumentTabBar";
import DocumentHost from "../components/workspace/DocumentHost";
import { GitCommitVertical } from "lucide-react";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import { RightPanelContext } from "../contexts/rightPanel";
import { useCapabilities } from "../hooks/useCapabilities";
import { AssistantProvider } from "../contexts/AssistantProvider";
import AssistantPanel from "../components/assistant/AssistantPanel";

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, disconnect, api } = useConnection();
  const navigate = useNavigate();
  const [revealed, setRevealed] = useState(false);
  const { sidebarOpen } = useWorkspace();
  const [rightPanel, setRightPanel] = useState<HTMLElement | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const capabilities = useCapabilities();

  useEffect(() => {
    if (!isConnected) {
      navigate("/");
    }
  }, [isConnected, navigate]);

  if (!isConnected) return null;

  async function disconnectNow() {
    await disconnect();
    navigate("/");
  }

  // Disconnecting drops an open transaction, so that asks first
  async function handleDisconnect() {
    if (capabilities.transactions && (await api.transactionOpen().catch(() => false))) setConfirmDisconnect(true);
    else await disconnectNow();
  }

  return (
    <DocumentsProvider>
    <AssistantProvider>
    <div className="h-full flex flex-col text-foreground">
      {/* Each part is a card of its own, with a small gap between them */}
      <div className="flex flex-1 min-h-0 gap-1.5">
        {/* Open or hidden, nothing in between; hidden, it also gives back the gap */}
        <aside
          inert={!sidebarOpen}
          className={cn(
            "relative shrink-0 select-none rounded-lg bg-sidebar transition-[width,margin] duration-300 ease-in-out overflow-hidden",
            sidebarOpen ? "w-64 border border-border" : "-mr-1.5 w-0",
          )}
        >
          <div
            className={cn(
              "absolute inset-y-0 left-0 w-64 flex flex-col transition-opacity duration-200",
              sidebarOpen ? "opacity-100 delay-75" : "opacity-0",
            )}
          >
            <div className="shrink-0 flex items-center px-2 pt-2 pb-1">
              <DatabaseSwitcher />
            </div>

            {/* The explorer has its own header, filter and scrolling list */}
            <div className="min-h-0 flex-1">
              <SchemaTree />
            </div>
          </div>

        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card">
          <RightPanelContext.Provider value={rightPanel}>
            <DocumentTabBar />
            <div className="min-h-0 flex-1">
              <DocumentHost />
            </div>
          </RightPanelContext.Provider>
        </main>
        {/* Filled by the page (query library, activity detail). `contents` so the
            portaled card is the flex item and can animate like the sidebar. */}
        <div ref={setRightPanel} className="contents empty:hidden" />
        <AssistantPanel />
      </div>

      <StatusBar
        connectionInfo={connectionInfo}
        revealed={revealed}
        onToggleReveal={() => setRevealed((v) => !v)}
        onDisconnect={handleDisconnect}
      />
      <ConfirmDialog
        open={confirmDisconnect}
        onClose={() => setConfirmDisconnect(false)}
        onConfirm={() => {
          setConfirmDisconnect(false);
          disconnectNow();
        }}
        icon={<GitCommitVertical />}
        tone="destructive"
        title="Disconnect with a transaction open?"
        description="Its changes haven't been committed, so disconnecting rolls them back."
        confirmLabel="Roll back and disconnect"
      />
    </div>
    </AssistantProvider>
    </DocumentsProvider>
  );
}
