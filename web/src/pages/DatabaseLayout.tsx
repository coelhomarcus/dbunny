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
    <div className="h-full flex flex-col bg-zinc-950 text-white">
      {isMac && <div className="h-8 w-full shrink-0" onMouseDown={startDrag} />}
      <div className="flex flex-1 overflow-hidden p-2 gap-2">
        <aside
          className={`shrink-0 bg-zinc-900 border border-zinc-800/60 rounded-xl shadow-xl flex flex-col select-none transition-all duration-300 ease-in-out overflow-hidden ${
            sidebarExpanded ? "w-64" : "w-12"
          }`}
        >
          {sidebarExpanded ? (
            <>
              <div className="px-3 pt-2.5 pb-2 flex items-center justify-between shrink-0">
                <button
                  onClick={() => setRevealed((v) => !v)}
                  className="flex-1 min-w-0 flex items-center gap-2 group text-left"
                  title={revealed ? "Hide connection details" : "Show connection details"}
                >
                  <div className="min-w-0 flex-1">
                    <span className="block text-sm text-white truncate font-medium leading-tight">
                      {savedConnectionName || connectionInfo?.database || "Connection"}
                    </span>
                    <span className="block text-[11px] text-zinc-500 truncate font-mono mt-0.5">
                      {revealed
                        ? `${connectionInfo?.database} @ ${connectionInfo?.host}:${connectionInfo?.port}`
                        : `${maskedDb} @ ${maskedHost}:${maskedPort}`}
                    </span>
                  </div>
                  <span className="text-zinc-600 group-hover:text-zinc-400 transition-colors shrink-0">
                    {revealed ? <EyeOff size={13} /> : <Eye size={13} />}
                  </span>
                </button>
                <button
                  onClick={() => setSidebarExpanded(false)}
                  className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors shrink-0 ml-1"
                  title="Collapse sidebar"
                >
                  <PanelLeftClose size={14} />
                </button>
              </div>

              <div className="px-2 py-1 border-t border-zinc-800/60">
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

              <div className="flex-1 overflow-y-auto py-1 border-t border-zinc-800/60">
                <SchemaTree />
              </div>

              <div className="px-2 py-2 border-t border-zinc-800/60">
                <button
                  onClick={handleDisconnect}
                  className="flex items-center gap-2 w-full px-2.5 py-2 rounded-md text-sm text-zinc-500 hover:text-red-400 hover:bg-zinc-800/40 transition-colors"
                >
                  <LogOut size={14} className="shrink-0" />
                  Disconnect
                </button>
              </div>
            </>
          ) : (
            <div className="flex flex-col items-center py-2.5 gap-1 h-full">
              <button
                onClick={() => setSidebarExpanded(true)}
                className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                title="Expand sidebar"
              >
                <PanelLeftOpen size={16} />
              </button>
              <NavLink
                to="/db/query"
                className={({ isActive }) =>
                  `p-1.5 rounded transition-colors ${
                    isActive
                      ? "text-white bg-zinc-800"
                      : "text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800"
                  }`
                }
                title="Query"
              >
                <Terminal size={16} />
              </NavLink>
              <div className="flex-1" />
              <button
                onClick={handleDisconnect}
                className="p-1.5 rounded text-zinc-600 hover:text-red-400 hover:bg-zinc-800 transition-colors"
                title="Disconnect"
              >
                <LogOut size={14} />
              </button>
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
