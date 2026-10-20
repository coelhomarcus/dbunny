import { useState } from "react";
import { CircleCheck, Download, RotateCw, TriangleAlert, X } from "lucide-react";
import Spinner from "./ui/Spinner";
import { dismissUpdate, installUpdate, RELEASES_URL, restartApp, useUpdate } from "../lib/updater";
import { openExternal } from "../lib/openExternal";
import { isWindows } from "../lib/platform";
import { cn } from "../lib/cn";
import Button from "./ui/Button";
import ReleaseNotesDialog from "./ReleaseNotesDialog";

/**
 * Card in the bottom-right corner while an update needs the person: one is available,
 * downloading, installed and waiting for a restart, or failed to install. "Later" hides it
 * until the app opens again; the status bar brings it back.
 */
export default function UpdateNotice() {
  const { state, dismissed } = useUpdate();
  const [notesOpen, setNotesOpen] = useState(false);
  const visible =
    (state.status === "available" && !dismissed) ||
    state.status === "downloading" ||
    (state.status === "ready" && !dismissed) ||
    (state.status === "error" && state.during === "install");
  if (!visible) return null;

  return (
    <div
      role="status"
      className="fixed bottom-10 right-4 z-40 w-80 rounded-xl border border-border bg-popover p-3.5 text-popover-foreground shadow-xl animate-dialog-in"
    >
      {state.status === "available" && (
        <>
          <div className="flex items-start gap-2.5">
            <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-primary/12 text-primary [&_svg]:size-3.5">
              <Download />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-semibold">DBunny {state.version} is available</div>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {isWindows ? "DBunny closes while it installs, then opens again." : "It installs in the background; restart when you're ready."}
              </p>
            </div>
            <Button variant="ghost-muted" size="icon-xs" className="-mr-1 -mt-1" aria-label="Hide" onClick={dismissUpdate}>
              <X />
            </Button>
          </div>
          <div className="mt-3 flex items-center justify-end gap-1.5">
            <Button variant="ghost-muted" size="sm" onClick={() => setNotesOpen(true)}>
              What's new
            </Button>
            <Button variant="outline" size="sm" onClick={dismissUpdate}>
              Later
            </Button>
            <Button variant="primary" size="sm" onClick={installUpdate}>
              Update
            </Button>
          </div>
        </>
      )}

      {state.status === "downloading" && (
        <>
          <div className="flex items-center gap-2.5 text-[13px] [&_svg]:size-4">
            <Spinner className="text-muted-foreground" />
            <span className="min-w-0 flex-1 font-medium">Downloading DBunny {state.version}</span>
            {state.progress !== null && <span className="tabular-nums text-muted-foreground">{Math.round(state.progress * 100)}%</span>}
          </div>
          <div className="mt-3 h-1 overflow-hidden rounded-full bg-foreground/10">
            <div
              className={cn("h-full rounded-full bg-primary transition-[width]", state.progress === null && "w-1/3 animate-pulse")}
              style={state.progress !== null ? { width: `${Math.round(state.progress * 100)}%` } : undefined}
            />
          </div>
        </>
      )}

      {state.status === "ready" && (
        <>
          <div className="flex items-start gap-2.5">
            <CircleCheck size={16} className="mt-0.5 shrink-0 text-success-foreground" />
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-semibold">DBunny {state.version} is installed</div>
              <p className="mt-0.5 text-xs text-muted-foreground">Restart to start using it. Open tabs and connections close.</p>
            </div>
          </div>
          <div className="mt-3 flex justify-end gap-1.5">
            <Button variant="outline" size="sm" onClick={dismissUpdate}>
              Later
            </Button>
            <Button variant="primary" size="sm" onClick={restartApp}>
              <RotateCw /> Restart now
            </Button>
          </div>
        </>
      )}

      {state.status === "error" && (
        <>
          <div className="flex items-start gap-2.5">
            <TriangleAlert size={16} className="mt-0.5 shrink-0 text-destructive-foreground" />
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-semibold">The update couldn't be installed</div>
              <p className="mt-0.5 select-text break-words text-xs text-muted-foreground">{state.message}</p>
            </div>
          </div>
          <div className="mt-3 flex justify-end gap-1.5">
            <Button variant="ghost-muted" size="sm" onClick={dismissUpdate}>
              Close
            </Button>
            <Button variant="outline" size="sm" onClick={() => openExternal(RELEASES_URL)}>
              Download manually
            </Button>
          </div>
        </>
      )}
      {(state.status === "available" || state.status === "downloading" || state.status === "ready") && (
        <ReleaseNotesDialog open={notesOpen} version={state.version} onClose={() => setNotesOpen(false)} />
      )}
    </div>
  );
}
