import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import Tooltip from "../components/Tooltip";
import Button from "../components/ui/Button";
import StatusBar from "../components/layout/StatusBar";
import { HeaderTitle } from "../components/layout/PageHeader";
import { cn } from "../lib/cn";
import { Terminal, Eye, EyeOff, PanelLeftClose, PanelLeftOpen, Folder } from "lucide-react";
import type { SchemaInfo } from "@/types";

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, savedConnectionName, disconnect, api } = useConnection();
  const navigate = useNavigate();
  const [revealed, setRevealed] = useState(false);
  const [sidebarExpanded, setSidebarExpanded] = useState(true);
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [schemaToExpand, setSchemaToExpand] = useState<string | null>(null);

  useEffect(() => {
    if (!isConnected) {
      navigate("/");
    }
  }, [isConnected, navigate]);

  useEffect(() => {
    if (isConnected) {
      api.getSchemas().then(setSchemas).catch(() => {});
    }
  }, [isConnected, api]);

  if (!isConnected) return null;

  async function handleDisconnect() {
    await disconnect();
    navigate("/");
  }

  const navItemClass = ({ isActive }: { isActive: boolean }) =>
    cn(
      "flex h-8 items-center gap-2 rounded-lg px-2.5 text-sm transition-colors",
      isActive ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
    );

  return (
    <div className="h-full flex flex-col bg-card text-foreground">
      <div className="flex flex-1 min-h-0">
        <aside
          className={cn(
            "relative shrink-0 border-r border-border select-none transition-[width] duration-300 ease-in-out overflow-hidden",
            sidebarExpanded ? "w-64" : "w-12",
          )}
        >
          {/* Expanded view */}
          <div
            className={cn(
              "absolute inset-y-0 left-0 w-64 flex flex-col transition-opacity duration-200",
              sidebarExpanded ? "opacity-100 delay-75" : "opacity-0 pointer-events-none",
            )}
          >
            {/* Same height as the page headers, so the border lines up */}
            <div className="h-12 shrink-0 flex items-center gap-1 border-b border-border pl-3 pr-2">
              <HeaderTitle label="Connection" className="flex-1">
                {savedConnectionName || connectionInfo?.database || "Connection"}
              </HeaderTitle>
              <Button
                variant="ghost-muted"
                size="icon-sm"
                onClick={() => setRevealed((v) => !v)}
                title={revealed ? "Hide connection details" : "Show connection details"}
              >
                {revealed ? <EyeOff /> : <Eye />}
              </Button>
              <Button variant="ghost-muted" size="icon-sm" onClick={() => setSidebarExpanded(false)} title="Collapse sidebar">
                <PanelLeftClose />
              </Button>
            </div>

            <nav className="p-2">
              <NavLink to="/db/query" className={navItemClass}>
                <Terminal size={15} className="shrink-0" />
                Query
              </NavLink>
            </nav>

            <div className="px-3 pt-1 pb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground/80">
              Explorer
            </div>
            <div className="flex-1 overflow-y-auto pb-2">
              <SchemaTree schemaToExpand={schemaToExpand} onSchemaExpanded={() => setSchemaToExpand(null)} />
            </div>
          </div>

          {/* Collapsed view: a narrow icon rail */}
          <div
            className={cn(
              "absolute inset-0 flex flex-col items-center gap-1 transition-opacity duration-200",
              sidebarExpanded ? "opacity-0 pointer-events-none" : "opacity-100 delay-75",
            )}
          >
            <div className="h-12 w-full shrink-0 flex items-center justify-center border-b border-border">
              <Tooltip label="Expand sidebar">
                <Button variant="ghost-muted" size="icon-sm" onClick={() => setSidebarExpanded(true)} aria-label="Expand sidebar">
                  <PanelLeftOpen />
                </Button>
              </Tooltip>
            </div>
            <div className="pt-1" />
            <Tooltip label="Query">
              <NavLink
                to="/db/query"
                className={({ isActive }) =>
                  cn(
                    "size-7 flex items-center justify-center rounded-lg transition-colors",
                    isActive ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                  )
                }
              >
                <Terminal size={15} />
              </NavLink>
            </Tooltip>
            {schemas.length > 0 && (
              <>
                <div className="w-5 border-t border-border my-0.5" />
                {schemas.map((s) => (
                  <Tooltip key={s.name} label={s.name}>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      className="text-warning-foreground/80 hover:text-warning-foreground"
                      aria-label={s.name}
                      onClick={() => {
                        setSchemaToExpand(s.name);
                        setSidebarExpanded(true);
                      }}
                    >
                      <Folder />
                    </Button>
                  </Tooltip>
                ))}
              </>
            )}
          </div>
        </aside>

        <main className="flex-1 min-w-0 overflow-hidden">
          <Outlet />
        </main>
      </div>

      <StatusBar connectionInfo={connectionInfo} revealed={revealed} onDisconnect={handleDisconnect} />
    </div>
  );
}
