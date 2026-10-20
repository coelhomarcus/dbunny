import { useState } from "react";
import { CircleCheck, Download, RotateCw, TriangleAlert, X } from "lucide-react";
import Spinner from "./ui/Spinner";
import { dismissUpdate, installUpdate, RELEASES_URL, restartApp, useUpdate } from "../lib/updater";
import { openExternal } from "../lib/openExternal";
import { isWindows } from "../lib/platform";
import { cn } from "../lib/cn";
import Button from "./ui/Button";
import ReleaseNotesDialog from "./ReleaseNotesDialog";
import { useT } from "../i18n";

/**
 * Card in the bottom-right corner while an update needs the person: one is available,
 * downloading, installed and waiting for a restart, or failed to install. "Later" hides it
 * until the app opens again; the status bar brings it back.
 */
export default function UpdateNotice() {
  const t = useT();
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
              <div className="text-[13px] font-semibold">{t("ui.updateNotice.available", { version: state.version })}</div>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {isWindows ? t("ui.updateNotice.installsWindows") : t("ui.updateNotice.installsBackground")}
              </p>
            </div>
            <Button variant="ghost-muted" size="icon-xs" className="-mr-1 -mt-1" aria-label={t("ui.updateNotice.hide")} onClick={dismissUpdate}>
              <X />
            </Button>
          </div>
          <div className="mt-3 flex items-center justify-end gap-1.5">
            <Button variant="ghost-muted" size="sm" onClick={() => setNotesOpen(true)}>
              {t("ui.updateNotice.whatsNew")}
            </Button>
            <Button variant="outline" size="sm" onClick={dismissUpdate}>
              {t("ui.updateNotice.later")}
            </Button>
            <Button variant="primary" size="sm" onClick={installUpdate}>
              {t("ui.updateNotice.update")}
            </Button>
          </div>
        </>
      )}

      {state.status === "downloading" && (
        <>
          <div className="flex items-center gap-2.5 text-[13px] [&_svg]:size-4">
            <Spinner className="text-muted-foreground" />
            <span className="min-w-0 flex-1 font-medium">{t("ui.updateNotice.downloading", { version: state.version })}</span>
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
              <div className="text-[13px] font-semibold">{t("ui.updateNotice.installed", { version: state.version })}</div>
              <p className="mt-0.5 text-xs text-muted-foreground">{t("ui.updateNotice.restartHint")}</p>
            </div>
          </div>
          <div className="mt-3 flex justify-end gap-1.5">
            <Button variant="outline" size="sm" onClick={dismissUpdate}>
              {t("ui.updateNotice.later")}
            </Button>
            <Button variant="primary" size="sm" onClick={restartApp}>
              <RotateCw /> {t("ui.updateNotice.restartNow")}
            </Button>
          </div>
        </>
      )}

      {state.status === "error" && (
        <>
          <div className="flex items-start gap-2.5">
            <TriangleAlert size={16} className="mt-0.5 shrink-0 text-destructive-foreground" />
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-semibold">{t("ui.updateNotice.installFailed")}</div>
              <p className="mt-0.5 select-text break-words text-xs text-muted-foreground">{state.message}</p>
            </div>
          </div>
          <div className="mt-3 flex justify-end gap-1.5">
            <Button variant="ghost-muted" size="sm" onClick={dismissUpdate}>
              {t("common.close")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => openExternal(RELEASES_URL)}>
              {t("ui.updateNotice.downloadManually")}
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
