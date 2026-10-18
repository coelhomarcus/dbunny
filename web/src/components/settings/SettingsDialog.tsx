import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { getVersion } from "@tauri-apps/api/app";
import {
  ArrowUpRight,
  CircleCheck,
  Code,
  Download,
  Info,
  Loader,
  Monitor,
  Moon,
  RefreshCw,
  RotateCw,
  Settings2,
  ShieldCheck,
  Sun,
  TriangleAlert,
  X,
} from "lucide-react";
import { cn } from "../../lib/cn";
import { useSettings } from "../../lib/settings";
import { useTheme, type ThemePreference } from "../../lib/theme";
import { checkForUpdates, installUpdate, RELEASES_URL, restartApp, useUpdate } from "../../lib/updater";
import { openExternal } from "../../lib/openExternal";
import { isWindows } from "../../lib/platform";
import type { SettingsSection } from "../../contexts/settingsDialog";
import Button from "../ui/Button";
import ReleaseNotesDialog from "../ReleaseNotesDialog";
import Switch from "../ui/Switch";
import SegmentedTabs from "../ui/SegmentedTabs";

const SECTIONS: { id: SettingsSection; label: string; icon: ReactNode }[] = [
  { id: "general", label: "General", icon: <Settings2 /> },
  { id: "editor", label: "Query editor", icon: <Code /> },
  { id: "updates", label: "Updates", icon: <RefreshCw /> },
  { id: "about", label: "About", icon: <Info /> },
];

const REPO_URL = "https://github.com/coelhomarcus/dbunny";

/** One setting: what it is and what it does on the left, its control on the right. */
function Row({ title, hint, children }: { title: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-border py-3.5 last:border-b-0">
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{title}</div>
        {hint && <div className="mt-0.5 text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return <h2 className="mb-1 text-[15px] font-semibold">{children}</h2>;
}

function useAppVersion() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(null));
  }, []);
  return version;
}

function GeneralSection() {
  const { preference, setPreference } = useTheme();
  return (
    <>
      <SectionTitle>General</SectionTitle>
      <Row title="Theme" hint="System follows your computer's light or dark appearance.">
        <SegmentedTabs<ThemePreference>
          value={preference}
          onChange={setPreference}
          options={[
            { value: "system", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Monitor /> System</span> },
            { value: "light", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Sun /> Light</span> },
            { value: "dark", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Moon /> Dark</span> },
          ]}
        />
      </Row>
    </>
  );
}

function EditorSection() {
  const [settings, update] = useSettings();
  return (
    <>
      <SectionTitle>Query editor</SectionTitle>
      <Row
        title="Safe mode on by default"
        hint="Asks before statements that drop, truncate, or change every row. The shield in the editor still turns it off for a connection."
      >
        <Switch checked={settings.safeModeByDefault} onChange={(v) => update({ safeModeByDefault: v })} aria-label="Safe mode on by default" />
      </Row>
    </>
  );
}

function UpdatesSection() {
  const [settings, update] = useSettings();
  const { state } = useUpdate();
  const version = useAppVersion();
  const [notesVersion, setNotesVersion] = useState<string | null>(null);

  const status = (() => {
    switch (state.status) {
      case "checking":
        return { icon: <Loader className="animate-spin" />, text: "Looking for a new version…" };
      case "up-to-date":
        return { icon: <CircleCheck className="text-success-foreground" />, text: "DBunny is up to date." };
      case "available":
        return { icon: <Download className="text-primary" />, text: `DBunny ${state.version} is available.` };
      case "downloading":
        return {
          icon: <Loader className="animate-spin" />,
          text: `Downloading ${state.version}${state.progress !== null ? ` · ${Math.round(state.progress * 100)}%` : "…"}`,
        };
      case "ready":
        return { icon: <CircleCheck className="text-success-foreground" />, text: `${state.version} is installed. Restart to use it.` };
      case "error":
        return {
          icon: <TriangleAlert className="text-destructive-foreground" />,
          text: `${state.during === "check" ? "Couldn't look for updates" : "Couldn't install the update"}: ${state.message}`,
        };
      default:
        return null;
    }
  })();

  return (
    <>
      <SectionTitle>Updates</SectionTitle>
      <Row title="Current version" hint={version ? `DBunny ${version}` : "Unknown in the browser preview"}>
        <Button variant="ghost-muted" size="sm" disabled={!version} onClick={() => version && setNotesVersion(version)}>
          Release notes
        </Button>
      </Row>
      <Row title="Check for updates automatically" hint="When the app opens and every few hours. Updates are downloaded only when you choose to.">
        <Switch checked={settings.checkForUpdates} onChange={(v) => update({ checkForUpdates: v })} aria-label="Check for updates automatically" />
      </Row>

      <div className="mt-4 rounded-lg border border-border bg-muted/50 p-3.5">
        <div className="flex items-center gap-3">
          <div className="flex min-w-0 flex-1 items-center gap-2 text-[13px] [&_svg]:size-4 [&_svg]:shrink-0">
            {status ? (
              <>
                {status.icon}
                <span className="min-w-0 break-words">{status.text}</span>
              </>
            ) : (
              <span className="text-muted-foreground">Check whether a newer version is out.</span>
            )}
          </div>
          {state.status === "available" ? (
            <div className="flex shrink-0 gap-1.5">
              <Button variant="ghost-muted" size="sm" onClick={() => setNotesVersion(state.version)}>
                What's new
              </Button>
              <Button variant="primary" size="sm" onClick={installUpdate}>
                <Download /> Update
              </Button>
            </div>
          ) : state.status === "ready" ? (
            <Button variant="primary" size="sm" onClick={restartApp}>
              <RotateCw /> Restart now
            </Button>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={() => checkForUpdates({ manual: true })}
              disabled={state.status === "checking" || state.status === "downloading"}
            >
              <RefreshCw /> Check now
            </Button>
          )}
        </div>
        {state.status === "downloading" && (
          <div className="mt-3 h-1 overflow-hidden rounded-full bg-foreground/10">
            <div
              className={cn("h-full rounded-full bg-primary transition-[width]", state.progress === null && "w-1/3 animate-pulse")}
              style={state.progress !== null ? { width: `${Math.round(state.progress * 100)}%` } : undefined}
            />
          </div>
        )}
        {state.status === "available" && isWindows && (
          <p className="mt-2 text-xs text-muted-foreground">DBunny closes while the update installs, then opens again.</p>
        )}
      </div>
      <ReleaseNotesDialog open={notesVersion !== null} version={notesVersion ?? ""} onClose={() => setNotesVersion(null)} />
    </>
  );
}

function AboutSection() {
  const version = useAppVersion();
  const links: { label: string; url: string }[] = [
    { label: "Source code on GitHub", url: REPO_URL },
    { label: "Releases and downloads", url: RELEASES_URL },
    { label: "Report a problem", url: `${REPO_URL}/issues/new` },
  ];
  return (
    <>
      <div className="flex items-center gap-4 pb-5">
        <img src="/icon.svg" alt="" className="size-14 invert dark:invert-0" />
        <div>
          <h2 className="text-lg font-semibold leading-tight">DBunny</h2>
          <p className="text-[13px] text-muted-foreground">{version ? `Version ${version}` : "A fast, lightweight desktop client for PostgreSQL."}</p>
        </div>
      </div>
      <div className="flex flex-col gap-1">
        {links.map((link) => (
          <button
            key={link.url}
            type="button"
            onClick={() => openExternal(link.url)}
            className="flex items-center justify-between rounded-lg px-3 py-2 text-left text-[13px] cursor-pointer hover:bg-accent"
          >
            {link.label}
            <ArrowUpRight size={14} className="text-muted-foreground" />
          </button>
        ))}
      </div>
      <p className="mt-5 flex items-center gap-1.5 text-xs text-muted-foreground">
        <ShieldCheck size={13} /> Saved connections are encrypted on this computer.
      </p>
    </>
  );
}

/** The Settings window: sections on the left, the chosen one on the right. Escape closes it. */
export default function SettingsDialog({
  open,
  section,
  onSection,
  onClose,
}: {
  open: boolean;
  section: SettingsSection;
  onSection: (section: SettingsSection) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] animate-fade-in" onMouseDown={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="relative flex h-[min(560px,100%)] w-full max-w-3xl overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl animate-dialog-in"
      >
        <nav className="flex w-52 shrink-0 flex-col gap-0.5 border-r border-border bg-sidebar p-2">
          <div className="px-2.5 pb-2 pt-1.5 text-[13px] font-semibold">Settings</div>
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              aria-current={section === s.id ? "page" : undefined}
              onClick={() => onSection(s.id)}
              className={cn(
                "flex h-8 items-center gap-2 rounded-lg px-2.5 text-left text-sm transition-colors cursor-pointer [&_svg]:size-[15px] [&_svg]:shrink-0",
                section === s.id ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
            >
              {s.icon}
              {s.label}
            </button>
          ))}
        </nav>
        <div className="relative min-w-0 flex-1 overflow-y-auto px-7 py-6">
          <Button variant="ghost-muted" size="icon-xs" className="absolute right-3 top-3" aria-label="Close settings" onClick={onClose}>
            <X />
          </Button>
          {section === "general" && <GeneralSection />}
          {section === "editor" && <EditorSection />}
          {section === "updates" && <UpdatesSection />}
          {section === "about" && <AboutSection />}
        </div>
      </div>
    </div>,
    document.body,
  );
}
