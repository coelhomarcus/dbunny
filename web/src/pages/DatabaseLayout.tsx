import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useWorkspace } from "../contexts/workspace";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import Tooltip from "../components/Tooltip";
import Button from "../components/ui/Button";
import StatusBar from "../components/layout/StatusBar";
import DatabaseSwitcher from "../components/layout/DatabaseSwitcher";
import { cn } from "../lib/cn";
import { Terminal, Folder, Activity, Users } from "lucide-react";
import type { SchemaInfo } from "@/types";

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, disconnect, api } = useConnection();
  const navigate = useNavigate();
  const [revealed, setRevealed] = useState(false);
  const { sidebarOpen: sidebarExpanded, setSidebarOpen: setSidebarExpanded } = useWorkspace();
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
    <div className="h-full flex flex-col text-foreground">
      <div className="flex flex-1 min-h-0">
        <aside
          className={cn(
            // Same colour as the title bar above it: together they form the window frame
            "relative shrink-0 select-none transition-[width] duration-300 ease-in-out overflow-hidden",
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
            <div className="h-10 shrink-0 flex items-center pl-3 pr-2">
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
            <div className="pt-2" />
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
            <Tooltip label="Activity">
              <NavLink
                to="/db/activity"
                aria-label="Activity"
                className={({ isActive }) =>
                  cn(
                    "size-7 flex items-center justify-center rounded-lg transition-colors",
                    isActive ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                  )
                }
              >
                <Activity size={15} />
              </NavLink>
            </Tooltip>
            <Tooltip label="Roles">
              <NavLink
                to="/db/roles"
                aria-label="Roles"
                className={({ isActive }) =>
                  cn(
                    "size-7 flex items-center justify-center rounded-lg transition-colors",
                    isActive ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                  )
                }
              >
                <Users size={15} />
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

        <main className="flex-1 min-w-0 bg-card rounded-tl-[10px] border-l border-t border-frame-line overflow-hidden">
          <Outlet />
        </main>
      </div>

      <StatusBar
        connectionInfo={connectionInfo}
        revealed={revealed}
        onToggleReveal={() => setRevealed((v) => !v)}
        onDisconnect={handleDisconnect}
      />
    </div>
  );
}
