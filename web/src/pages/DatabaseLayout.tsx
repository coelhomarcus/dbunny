import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import { Terminal, Eye, EyeOff, LogOut, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMac } from "../lib/platform";

async function startDrag(e: React.MouseEvent) {
  e.preventDefault();
  await getCurrentWindow().startDragging();
}

export default function DatabaseLayout() {
  const { isConnected, connectionInfo, savedConnectionName, disconnect } = useConnection();
  const navigate = useNavigate();
  const [revealed, setRevealed] = useState(false);
  const [sidebarExpanded, setSidebarExpanded] = useState(true);

  useEffect(() => {
    if (!isConnected) {
      navigate("/");
    }
  }, [isConnected, navigate]);

  if (!isConnected) return null;

  async function handleDisconnect() {
    await disconnect();
    navigate("/");
  }

  const maskedHost = connectionInfo?.host.replace(/./g, "•") ?? "••••••••";
  const maskedPort = connectionInfo?.port.toString().replace(/./g, "•") ?? "••••";
  const maskedDb = connectionInfo?.database.replace(/./g, "•") ?? "••••••••";

  return (
    <div className="h-screen flex flex-col bg-zinc-950 text-white">
      {isMac && <div className="h-8 w-full shrink-0" onMouseDown={startDrag} />}
      <div className="flex flex-1 overflow-hidden p-2 gap-2">
        <aside
          className={`shrink-0 bg-zinc-900 border border-zinc-800/60 rounded-xl shadow-xl flex flex-col select-none transition-all duration-300 ease-in-out overflow-hidden ${
            sidebarExpanded ? "w-64" : "w-12"
          }`}
        >
          {sidebarExpanded ? (
            <>
              <div className="px-3 pt-3 pb-2.5 border-b border-zinc-800/60 space-y-2.5">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <img src="/icon.svg" className="size-6 shrink-0" />
                    <span className="text-base font-semibold text-white truncate">
                      DBunny
                    </span>
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      onClick={handleDisconnect}
                      className="p-1.5 rounded text-zinc-600 hover:text-red-400 hover:bg-zinc-800 transition-colors shrink-0"
                      title="Disconnect"
                    >
                      <LogOut size={14} />
                    </button>
                    <button
                      onClick={() => setSidebarExpanded(false)}
                      className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors shrink-0"
                      title="Minimize sidebar"
                    >
                      <PanelLeftClose size={14} />
                    </button>
                  </div>
                </div>

                <div className="pl-0.5 space-y-1">
                  <div className="flex items-center justify-between">
                    <span className="text-[11px] font-medium text-zinc-600 uppercase tracking-wider">
                      Connection
                    </span>
                    <button
                      onClick={() => setRevealed((v) => !v)}
                      className="p-1 rounded text-zinc-600 hover:text-zinc-400 hover:bg-zinc-800 transition-colors shrink-0"
                      title={revealed ? "Hide connection details" : "Show connection details"}
                    >
                      {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
                    </button>
                  </div>
                  {savedConnectionName && (
                    <span className="block text-sm text-white truncate font-medium">
                      {savedConnectionName}
                    </span>
                  )}
                  <span className="block text-xs text-zinc-500 truncate font-mono tracking-tight">
                    {revealed
                      ? `${connectionInfo?.database} @ ${connectionInfo?.host}:${connectionInfo?.port}`
                      : `${maskedDb} @ ${maskedHost}:${maskedPort}`}
                  </span>
                </div>
              </div>

              <div className="px-2 py-1.5 border-b border-zinc-800/60">
                <NavLink
                  to="/db/query"
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-2.5 py-2 rounded-md text-sm transition-colors ${
                      isActive
                        ? "bg-zinc-800 text-white"
                        : "text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/40"
                    }`
                  }
                >
                  <Terminal size={15} className="shrink-0" />
                  Query
                </NavLink>
              </div>

              <div className="flex-1 overflow-y-auto py-1">
                <SchemaTree />
              </div>
            </>
          ) : (
            <div className="flex flex-col items-center py-3 gap-3">
              <button
                onClick={() => setSidebarExpanded(true)}
                className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                title="Expand sidebar"
              >
                <PanelLeftOpen size={16} />
              </button>
              <img src="/icon.svg" className="size-5 opacity-60" />
            </div>
          )}
        </aside>

        <main className="flex-1 overflow-hidden">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
