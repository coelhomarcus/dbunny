import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import Tooltip from "../components/Tooltip";
import Button from "../components/ui/Button";
import ThemeToggle from "../components/ui/ThemeToggle";
import { Terminal, Eye, EyeOff, LogOut, PanelLeftClose, PanelLeftOpen, Folder } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMac } from "../lib/platform";
import { api } from "../lib/api";
import type { SchemaInfo } from "@/types";

async function startDrag(e: React.MouseEvent) {
  e.preventDefault();
  await getCurrentWindow().startDragging();
}

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, savedConnectionName, disconnect } = useConnection();
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
  }, [isConnected]);

  if (!isConnected) return null;

  async function handleDisconnect() {
    await disconnect();
    navigate("/");
  }

  const maskedHost = connectionInfo?.host.replace(/./g, "•") ?? "••••••••";
  const maskedPort = connectionInfo?.port.toString().replace(/./g, "•") ?? "••••";
  const maskedDb = connectionInfo?.database.replace(/./g, "•") ?? "••••••••";

  return (
    <div className="h-full flex flex-col bg-background text-foreground">
      {isMac && <div className="h-8 w-full shrink-0" onMouseDown={startDrag} />}
      <div className="flex flex-1 overflow-hidden p-2 gap-2">
        <aside
          className={`shrink-0 bg-card border border-border rounded-xl shadow-xs flex flex-col select-none transition-all duration-300 ease-in-out overflow-hidden ${
            sidebarExpanded ? "w-64" : "w-12"
          }`}
        >
          <div className="relative flex-1">
            {/* Expanded view */}
            <div className={`absolute inset-0 flex flex-col transition-opacity duration-200 ${
              sidebarExpanded ? "opacity-100 delay-75" : "opacity-0 pointer-events-none"
            }`}>
              <div className="px-3 pt-2.5 pb-2 flex items-center justify-between shrink-0">
                <button
                  onClick={() => setRevealed((v) => !v)}
                  className="flex-1 min-w-0 flex items-center gap-2 group text-left"
                  title={revealed ? "Hide connection details" : "Show connection details"}
                >
                  <div className="min-w-0 flex-1">
                    <span className="block text-sm text-foreground truncate font-medium leading-tight">
                      {savedConnectionName || connectionInfo?.database || "Connection"}
                    </span>
                    <span className="block text-[11px] text-muted-foreground truncate font-mono mt-0.5">
                      {revealed
                        ? `${connectionInfo?.database} @ ${connectionInfo?.host}:${connectionInfo?.port}`
                        : `${maskedDb} @ ${maskedHost}:${maskedPort}`}
                    </span>
                  </div>
                  <span className="text-muted-foreground/70 group-hover:text-muted-foreground transition-colors shrink-0">
                    {revealed ? <EyeOff size={13} /> : <Eye size={13} />}
                  </span>
                </button>
                <Button variant="ghost-muted" size="icon-sm" className="ml-1" onClick={() => setSidebarExpanded(false)} title="Collapse sidebar">
                  <PanelLeftClose />
                </Button>
              </div>

              <div className="px-2 py-1 border-t border-border">
                <NavLink
                  to="/db/query"
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-2.5 py-2 rounded-md text-sm transition-colors ${
                      isActive
                        ? "bg-accent text-foreground font-medium"
                        : "text-muted-foreground hover:text-foreground hover:bg-accent"
                    }`
                  }
                >
                  <Terminal size={15} className="shrink-0" />
                  Query
                </NavLink>
              </div>

              <div className="flex-1 overflow-y-auto py-1 border-t border-border">
                <SchemaTree
                  schemaToExpand={schemaToExpand}
                  onSchemaExpanded={() => setSchemaToExpand(null)}
                />
              </div>

              <div className="px-2 py-2 border-t border-border flex items-center gap-1">
                <button
                  onClick={handleDisconnect}
                  className="flex-1 flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-sm text-muted-foreground hover:text-destructive-foreground hover:bg-destructive/10 transition-colors cursor-pointer"
                >
                  <LogOut size={14} className="shrink-0" />
                  Disconnect
                </button>
                <ThemeToggle />
              </div>
            </div>

            {/* Collapsed view */}
            <div className={`absolute inset-0 flex flex-col items-center py-2.5 gap-1 transition-opacity duration-200 ${
              sidebarExpanded ? "opacity-0 pointer-events-none" : "opacity-100 delay-75"
            }`}>
              <Tooltip label="Expand sidebar">
                <Button variant="ghost-muted" size="icon-sm" onClick={() => setSidebarExpanded(true)} aria-label="Expand sidebar">
                  <PanelLeftOpen />
                </Button>
              </Tooltip>
              <Tooltip label="Query">
                <NavLink
                  to="/db/query"
                  className={({ isActive }) =>
                    `size-7 flex items-center justify-center rounded-lg transition-colors ${
                      isActive
                        ? "text-foreground bg-accent"
                        : "text-muted-foreground/70 hover:text-foreground hover:bg-accent"
                    }`
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
              <div className="flex-1" />
              <ThemeToggle side="right" />
              <Tooltip label="Disconnect">
                <Button variant="destructive-ghost" size="icon-sm" onClick={handleDisconnect} aria-label="Disconnect">
                  <LogOut />
                </Button>
              </Tooltip>
            </div>
          </div>
        </aside>

        <main className="flex-1 overflow-hidden">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
