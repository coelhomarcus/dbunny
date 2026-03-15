import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";
import { isWindows } from "../lib/platform";

function RestoreIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.2">
      <rect x="2.5" y="4" width="6" height="5.5" rx="0.5" />
      <path d="M4.5 4V2.5a.5.5 0 0 1 .5-.5H10a.5.5 0 0 1 .5.5V7a.5.5 0 0 1-.5.5H8.5" />
    </svg>
  );
}

function MaximizeIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.2">
      <rect x="1.5" y="1.5" width="8" height="8" rx="0.5" />
    </svg>
  );
}

export default function TitleBar() {
  if (!isWindows) return null;

  const [maximized, setMaximized] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    appWindow.isMaximized().then(setMaximized);

    let cancel = false;
    const promise = appWindow.onResized(async () => {
      if (!cancel) {
        setMaximized(await appWindow.isMaximized());
      }
    });
    return () => {
      cancel = true;
      promise.then((fn) => fn());
    };
  }, []);

  async function startDrag(e: React.MouseEvent) {
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    await appWindow.startDragging();
  }

  return (
    <div
      className="h-9 bg-zinc-950 flex items-center justify-between select-none shrink-0"
      onMouseDown={startDrag}
    >
      <div className="flex items-center gap-2 px-3 pointer-events-none">
        <img src="/icon.svg" className="size-4" />
        <span className="text-xs text-zinc-500 font-medium">DBunny</span>
      </div>

      <div className="flex h-full">
        <button
          onClick={() => appWindow.minimize()}
          className="h-full w-12 flex items-center justify-center text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors cursor-pointer"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={() => appWindow.toggleMaximize()}
          className="h-full w-12 flex items-center justify-center text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition-colors cursor-pointer"
        >
          {maximized ? <RestoreIcon /> : <MaximizeIcon />}
        </button>
        <button
          onClick={() => appWindow.close()}
          className="h-full w-12 flex items-center justify-center text-zinc-400 hover:bg-red-600 hover:text-white transition-colors cursor-pointer"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
