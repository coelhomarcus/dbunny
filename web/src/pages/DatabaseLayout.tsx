import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useWorkspace } from "../contexts/workspace";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import StatusBar from "../components/layout/StatusBar";
import DatabaseSwitcher from "../components/layout/DatabaseSwitcher";
import { cn } from "../lib/cn";
import { Terminal, Activity, Users, GitCommitVertical } from "lucide-react";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import { RightPanelContext } from "../contexts/rightPanel";

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, disconnect, api } = useConnection();
  const navigate = useNavigate();
  const [revealed, setRevealed] = useState(false);
  const { sidebarOpen } = useWorkspace();
  const [rightPanel, setRightPanel] = useState<HTMLElement | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);

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
    if (await api.transactionOpen().catch(() => false)) setConfirmDisconnect(true);
    else await disconnectNow();
  }

  const navItemClass = ({ isActive }: { isActive: boolean }) =>
    cn(
      "flex h-8 items-center gap-2 rounded-lg px-2.5 text-sm transition-colors",
      isActive ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
    );

  return (
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

            <nav className="px-2 pb-2">
              <NavLink to="/db/query" className={navItemClass}>
                <Terminal size={15} className="shrink-0" />
                Query
              </NavLink>
              <NavLink to="/db/activity" className={navItemClass}>
                <Activity size={15} className="shrink-0" />
                Activity
              </NavLink>
              <NavLink to="/db/roles" className={navItemClass}>
                <Users size={15} className="shrink-0" />
                Roles
              </NavLink>
            </nav>

            {/* The explorer has its own header, filter and scrolling list */}
            <div className="min-h-0 flex-1">
              <SchemaTree />
            </div>
          </div>

        </aside>

        <main className="flex-1 min-w-0 rounded-lg border border-border bg-card overflow-hidden">
          <RightPanelContext.Provider value={rightPanel}>
            <Outlet />
          </RightPanelContext.Provider>
        </main>
        {/* Filled by the page (e.g. the query library); hidden while empty */}
        <div ref={setRightPanel} className="flex w-80 shrink-0 overflow-hidden rounded-lg border border-border bg-card empty:hidden" />
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
  );
}
