import { Outlet, useNavigate, NavLink } from "react-router";
import { useConnection } from "../contexts/useConnection";
import { useEffect, useState } from "react";
import SchemaTree from "../components/SchemaTree";
import { Terminal, Eye, EyeOff, LogOut } from "lucide-react";
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
    <div className="h-screen flex flex-col  bg-zinc-950 text-white">
      {/* Drag region - only needed on macOS where title bar is overlay */}
      {isMac && <div className="h-8 w-full shrink-0" onMouseDown={startDrag} />}
      <div className="flex flex-1 overflow-hidden">
      {/* Sidebar */}
      <aside className="w-56 shrink-0 border-r border-t border-zinc-800/60 flex flex-col select-none">
        {/* Header / Connection info */}
        <div className="px-3 pt-3 pb-2.5 border-b border-zinc-800/60 space-y-2.5">
          {/* App name row */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <img src="/icon.svg" className="size-5 shrink-0" />
              <span className="text-sm font-semibold text-white truncate">
                DBunny
              </span>
            </div>
            <button
              onClick={handleDisconnect}
              className="p-1 rounded text-zinc-600 hover:text-red-400 hover:bg-zinc-800 transition-colors shrink-0"
              title="Disconnect"
            >
              <LogOut size={13} />
            </button>
          </div>

          {/* Connection details */}
          <div className="pl-0.5 space-y-1">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-medium text-zinc-600 uppercase tracking-wider">
                Connection
              </span>
              <button
                onClick={() => setRevealed((v) => !v)}
                className="p-1 rounded text-zinc-600 hover:text-zinc-400 hover:bg-zinc-800 transition-colors shrink-0"
                title={revealed ? "Hide connection details" : "Show connection details"}
              >
                {revealed ? <EyeOff size={13} /> : <Eye size={13} />}
              </button>
            </div>
            {savedConnectionName && (
              <span className="block text-[11px] text-white truncate font-medium">
                {savedConnectionName}
              </span>
            )}
            <span className="block text-[11px] text-zinc-500 truncate font-mono tracking-tight">
              {revealed
                ? `${connectionInfo?.database} @ ${connectionInfo?.host}:${connectionInfo?.port}`
                : `${maskedDb} @ ${maskedHost}:${maskedPort}`}
            </span>
          </div>
        </div>

        {/* Navigation */}
        <div className="px-2 py-1.5 border-b border-zinc-800/60">
          <NavLink
            to="/db/query"
            className={({ isActive }) =>
              `flex items-center gap-2 px-2.5 py-1.5 rounded-md text-xs transition-colors ${
                isActive
                  ? "bg-zinc-800 text-white"
                  : "text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/40"
              }`
            }
          >
            <Terminal size={13} className="shrink-0" />
            Query
          </NavLink>
        </div>

        {/* Schema tree */}
        <div className="flex-1 overflow-y-auto py-1">
          <SchemaTree />
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-hidden border-t border-zinc-800/60">
        <Outlet />
      </main>
      </div>
    </div>
  );
}
