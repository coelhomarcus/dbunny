import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { SettingsDialogContext, type SettingsSection } from "../../contexts/settingsDialog";
import { checkForUpdates, startUpdateChecks } from "../../lib/updater";
import { isMac } from "../../lib/platform";
import SettingsDialog from "./SettingsDialog";

/**
 * Settings for the whole app: opened from the status bar, Cmd/Ctrl+, or the macOS app menu,
 * which also has "Check for Updates…". Starts the automatic update checks too.
 */
export default function SettingsProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [section, setSection] = useState<SettingsSection>("general");

  const openSettings = useCallback((next?: SettingsSection) => {
    if (next) setSection(next);
    setOpen(true);
  }, []);
  const close = useCallback(() => setOpen(false), []);

  useEffect(() => startUpdateChecks(), []);

  // Cmd+, on macOS comes through the menu accelerator below; elsewhere it's a key press
  useEffect(() => {
    if (isMac) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key === ",") {
        e.preventDefault();
        openSettings();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openSettings]);

  useEffect(() => {
    if (!isMac) return;
    const unlisten = [
      listen("menu:settings", () => openSettings()),
      listen("menu:check-updates", () => {
        openSettings("updates");
        checkForUpdates({ manual: true });
      }),
    ];
    return () => {
      unlisten.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, [openSettings]);

  const value = useMemo(() => ({ openSettings }), [openSettings]);

  return (
    <SettingsDialogContext.Provider value={value}>
      {children}
      <SettingsDialog open={open} section={section} onSection={setSection} onClose={close} />
    </SettingsDialogContext.Provider>
  );
}
