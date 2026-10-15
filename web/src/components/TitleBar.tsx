import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Database, Minus, PanelLeft, Plus, X } from "lucide-react";
import { isMac, isWindows } from "../lib/platform";
import { cn } from "../lib/cn";
import { useWorkspace, type WorkspaceTab } from "../contexts/workspace";
import Button from "./ui/Button";

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

const appWindow = getCurrentWindow();

/** Only the empty parts of the bar move the window, not tabs or buttons. */
function isDragArea(target: EventTarget) {
  return !(target as HTMLElement).closest("button, [role='tab']");
}

function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    appWindow.isMaximized().then(setMaximized);
    let cancel = false;
    const promise = appWindow.onResized(async () => {
      if (!cancel) setMaximized(await appWindow.isMaximized());
    });
    return () => {
      cancel = true;
      promise.then((fn) => fn());
    };
  }, []);

  const control = "h-full w-12 flex items-center justify-center text-muted-foreground transition-colors cursor-pointer";
  return (
    <div className="flex h-full shrink-0 self-stretch">
      <button aria-label="Minimize" onClick={() => appWindow.minimize()} className={cn(control, "hover:bg-accent hover:text-foreground")}>
        <Minus size={14} />
      </button>
      <button
        aria-label={maximized ? "Restore" : "Maximize"}
        onClick={() => appWindow.toggleMaximize()}
        className={cn(control, "hover:bg-accent hover:text-foreground")}
      >
        {maximized ? <RestoreIcon /> : <MaximizeIcon />}
      </button>
      <button aria-label="Close" onClick={() => appWindow.close()} className={cn(control, "hover:bg-destructive hover:text-white")}>
        <X size={14} />
      </button>
    </div>
  );
}

function WindowTab({
  tab,
  active,
  onActivate,
  onClose,
}: {
  tab: WorkspaceTab;
  active: boolean;
  onActivate: () => void;
  onClose: () => void;
}) {
  const title = tab.title ?? "New Connection";
  return (
    <div
      role="tab"
      tabIndex={0}
      aria-selected={active}
      data-active={active}
      title={tab.subtitle ?? undefined}
      onMouseDown={(e) => {
        if (e.button === 0) onActivate();
      }}
      onAuxClick={(e) => {
        // Middle click closes, like a browser
        if (e.button === 1) {
          e.preventDefault();
          onClose();
        }
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onActivate();
        }
      }}
      className={cn(
        "joined-tab window-tab group flex min-w-[72px] max-w-[220px] flex-1 basis-0 items-center gap-2 pl-3 pr-1.5 pt-1.5 text-[13px] cursor-default outline-none",
        active
          ? "pb-[7px] text-foreground"
          : "pb-0.5 mb-1 rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground hover:transition-colors",
      )}
    >
      {tab.color ? (
        <span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: tab.color }} />
      ) : (
        <Database size={13} className={cn("shrink-0", tab.sessionId ? "text-primary" : "opacity-60")} />
      )}
      <span className={cn("min-w-0 flex-1 truncate", active && "font-medium")}>{title}</span>
      <button
        type="button"
        aria-label={`Close ${title}`}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        className={cn(
          "flex size-[18px] shrink-0 items-center justify-center rounded-full cursor-pointer transition-colors hover:bg-foreground/10 hover:text-foreground",
          active ? "text-muted-foreground" : "text-transparent group-hover:text-muted-foreground",
        )}
      >
        <X size={11} />
      </button>
    </div>
  );
}

/**
 * Window title bar: one tab per database connection, plus the window controls
 * (traffic lights on macOS sit on the left, drawn by the OS).
 */
export default function TitleBar() {
  const { tabs, activeTabId, activateTab, closeTab, openTab, cycleTab, sidebarOpen, toggleSidebar } = useWorkspace();

  // Cmd/Ctrl+T new tab, Ctrl+W close tab, Ctrl+Tab / Ctrl+Shift+Tab cycle, Cmd/Ctrl+1..9 jump
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (mod && !e.shiftKey && e.key.toLowerCase() === "t") {
        e.preventDefault();
        openTab();
      } else if (!isMac && e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "w") {
        // On macOS Cmd+W is a menu accelerator, see the listener below
        e.preventDefault();
        closeTab(activeTabId);
      } else if (e.ctrlKey && e.key === "Tab") {
        e.preventDefault();
        cycleTab(e.shiftKey ? -1 : 1);
      } else if (mod && /^[1-9]$/.test(e.key)) {
        e.preventDefault();
        const index = e.key === "9" ? tabs.length - 1 : Number(e.key) - 1;
        if (tabs[index]) activateTab(tabs[index].id);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [tabs, activeTabId, openTab, closeTab, cycleTab, activateTab]);

  // macOS: the native menu's "Close Tab" item (Cmd+W) emits this event
  useEffect(() => {
    if (!isMac) return;
    const unlisten = listen("menu:close-tab", () => closeTab(activeTabId));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [activeTabId, closeTab]);

  return (
    <div
      className="relative z-10 flex h-10 shrink-0 select-none"
      onMouseDown={(e) => {
        if (e.button === 0 && e.detail === 1 && isDragArea(e.target)) appWindow.startDragging();
      }}
      onDoubleClick={(e) => {
        if (isDragArea(e.target)) appWindow.toggleMaximize();
      }}
    >
      {/* Sidebar block: same width as the sidebar below it, so both read as one
          surface (traffic lights on macOS, collapse toggle at its right edge) */}
      <div
        className={cn(
          "flex shrink-0 items-center justify-end transition-[width] duration-300 ease-in-out",
          sidebarOpen ? "w-64 pr-2" : isMac ? "w-[7.5rem] pr-2" : "w-12 justify-center",
        )}
      >
        <Button
          variant="ghost-muted"
          size="icon-sm"
          onClick={toggleSidebar}
          title={sidebarOpen ? "Collapse sidebar" : "Expand sidebar"}
          aria-label={sidebarOpen ? "Collapse sidebar" : "Expand sidebar"}
        >
          <PanelLeft />
        </Button>
      </div>

      <div role="tablist" className="tab-strip window-tabs flex min-w-0 flex-1 items-end gap-1 pl-5 pr-2">
        {tabs.map((tab) => (
          <WindowTab
            key={tab.id}
            tab={tab}
            active={tab.id === activeTabId}
            onActivate={() => activateTab(tab.id)}
            onClose={() => closeTab(tab.id)}
          />
        ))}
        <Button variant="ghost-muted" size="icon-xs" className="mb-1.5 ml-0.5" onClick={openTab} title="New tab">
          <Plus />
        </Button>
      </div>
      {isWindows && <WindowControls />}
    </div>
  );
}
