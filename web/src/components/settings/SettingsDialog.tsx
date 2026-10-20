import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { getVersion } from "@tauri-apps/api/app";
import { ArrowUpRight, CircleCheck, Code, Download, Info, Monitor, Moon, RefreshCw, RotateCw, Settings2, ShieldCheck, Sun, TriangleAlert, X } from "lucide-react";
import Spinner from "../ui/Spinner";
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
import AssistantSection from "./AssistantSection";
import LanguagePicker from "./LanguagePicker";
import { useT, type StaticKey } from "../../i18n";
import AiIcon from "../ui/AiIcon";
import { Row, SectionTitle } from "./SettingsRows";
import SegmentedTabs from "../ui/SegmentedTabs";

const SECTIONS: { id: SettingsSection; labelKey: StaticKey; icon: ReactNode }[] = [
  { id: "general", labelKey: "settings.settingsDialog.general", icon: <Settings2 /> },
  { id: "editor", labelKey: "settings.settingsDialog.queryEditor", icon: <Code /> },
  { id: "assistant", labelKey: "assistant.assistantSection.aiAssistant", icon: <AiIcon /> },
  { id: "updates", labelKey: "settings.settingsDialog.updates", icon: <RefreshCw /> },
  { id: "about", labelKey: "settings.settingsDialog.about", icon: <Info /> },
];

const REPO_URL = "https://github.com/coelhomarcus/dbunny";

function useAppVersion() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(null));
  }, []);
  return version;
}

function GeneralSection() {
  const { preference, setPreference } = useTheme();
  const t = useT();
  return (
    <>
      <SectionTitle>{t("settings.settingsDialog.general")}</SectionTitle>
      <Row title={t("settings.settingsDialog.theme")} hint={t("settings.settingsDialog.systemFollowsYourComputersLight")}>
        <SegmentedTabs<ThemePreference>
          value={preference}
          onChange={setPreference}
          options={[
            { value: "system", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Monitor /> {t("settings.settingsDialog.themeSystem")}</span> },
            { value: "light", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Sun /> {t("settings.settingsDialog.themeLight")}</span> },
            { value: "dark", label: <span className="flex items-center gap-1.5 [&_svg]:size-3.5"><Moon /> {t("settings.settingsDialog.themeDark")}</span> },
          ]}
        />
      </Row>
      <Row title={t("settings.language.label")} hint={t("settings.language.hint")}>
        <LanguagePicker />
      </Row>
    </>
  );
}

function EditorSection() {
  const t = useT();
  const [settings, update] = useSettings();
  return (
    <>
      <SectionTitle>{t("settings.settingsDialog.queryEditor")}</SectionTitle>
      <Row
        title={t("settings.settingsDialog.safeModeOnByDefault")}
        hint={t("settings.settingsDialog.asksBeforeStatementsThatDrop")}
      >
        <Switch checked={settings.safeModeByDefault} onChange={(v) => update({ safeModeByDefault: v })} aria-label={t("settings.settingsDialog.safeModeOnByDefault")} />
      </Row>
    </>
  );
}

function UpdatesSection() {
  const t = useT();
  const [settings, update] = useSettings();
  const { state } = useUpdate();
  const version = useAppVersion();
  const [notesVersion, setNotesVersion] = useState<string | null>(null);

  const status = (() => {
    switch (state.status) {
      case "checking":
        return { icon: <Spinner />, text: t("settings.settingsDialog.lookingForANewVersion") };
      case "up-to-date":
        return { icon: <CircleCheck className="text-success-foreground" />, text: t("settings.settingsDialog.dbunnyIsUpToDate") };
      case "available":
        return { icon: <Download className="text-primary" />, text: t("settings.settingsDialog.available", { version: state.version }) };
      case "downloading":
        return {
          icon: <Spinner />,
          text: state.progress !== null ? t("settings.settingsDialog.downloadingProgress", { version: state.version, percent: Math.round(state.progress * 100) }) : t("settings.settingsDialog.downloading", { version: state.version }),
        };
      case "ready":
        return { icon: <CircleCheck className="text-success-foreground" />, text: t("settings.settingsDialog.installed", { version: state.version }) };
      case "error":
        return {
          icon: <TriangleAlert className="text-destructive-foreground" />,
          text: t(state.during === "check" ? "settings.settingsDialog.checkFailed" : "settings.settingsDialog.installFailed", { message: state.message }),
        };
      default:
        return null;
    }
  })();

  return (
    <>
      <SectionTitle>{t("settings.settingsDialog.updates")}</SectionTitle>
      <Row title={t("settings.settingsDialog.currentVersion")} hint={version ? `DBunny ${version}` : t("settings.settingsDialog.unknownVersion")}>
        <Button variant="ghost-muted" size="sm" disabled={!version} onClick={() => version && setNotesVersion(version)}>
          {t("settings.settingsDialog.releaseNotes")}
        </Button>
      </Row>
      <Row title={t("settings.settingsDialog.checkForUpdatesAutomatically")} hint={t("settings.settingsDialog.whenTheAppOpensAnd")}>
        <Switch checked={settings.checkForUpdates} onChange={(v) => update({ checkForUpdates: v })} aria-label={t("settings.settingsDialog.checkForUpdatesAutomatically")} />
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
              <span className="text-muted-foreground">{t("settings.settingsDialog.checkWhetherANewerVersion")}</span>
            )}
          </div>
          {state.status === "available" ? (
            <div className="flex shrink-0 gap-1.5">
              <Button variant="ghost-muted" size="sm" onClick={() => setNotesVersion(state.version)}>
                {t("settings.settingsDialog.whatsNew")}
              </Button>
              <Button variant="primary" size="sm" onClick={installUpdate}>
                <Download /> {t("ui.updateNotice.update")}
              </Button>
            </div>
          ) : state.status === "ready" ? (
            <Button variant="primary" size="sm" onClick={restartApp}>
              <RotateCw /> {t("ui.updateNotice.restartNow")}
            </Button>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={() => checkForUpdates({ manual: true })}
              disabled={state.status === "checking" || state.status === "downloading"}
            >
              <RefreshCw /> {t("settings.settingsDialog.checkNow")}
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
          <p className="mt-2 text-xs text-muted-foreground">{t("settings.settingsDialog.dbunnyClosesWhileTheUpdate")}</p>
        )}
      </div>
      <ReleaseNotesDialog open={notesVersion !== null} version={notesVersion ?? ""} onClose={() => setNotesVersion(null)} />
    </>
  );
}

function AboutSection() {
  const t = useT();
  const version = useAppVersion();
  const links: { label: string; url: string }[] = [
    { label: t("settings.settingsDialog.sourceCodeOnGithub"), url: REPO_URL },
    { label: t("settings.settingsDialog.releasesAndDownloads"), url: RELEASES_URL },
    { label: t("settings.settingsDialog.reportAProblem"), url: `${REPO_URL}/issues/new` },
  ];
  return (
    <>
      <div className="flex items-center gap-4 pb-5">
        <img src="/icon.svg" alt="" className="size-14 invert dark:invert-0" />
        <div>
          <h2 className="text-lg font-semibold leading-tight">DBunny</h2>
          <p className="text-[13px] text-muted-foreground">{version ? t("settings.settingsDialog.version", { version }) : t("settings.settingsDialog.tagline")}</p>
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
        <ShieldCheck size={13} /> {t("settings.settingsDialog.encrypted")}
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
  const t = useT();
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
        aria-label={t("settings.settingsDialog.settings")}
        className="relative flex h-[min(560px,100%)] w-full max-w-3xl overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl animate-dialog-in"
      >
        <nav className="flex w-52 shrink-0 flex-col gap-0.5 border-r border-border bg-sidebar p-2">
          <div className="px-2.5 pb-2 pt-1.5 text-[13px] font-semibold">{t("settings.settingsDialog.settings")}</div>
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
              {t(s.labelKey)}
            </button>
          ))}
        </nav>
        <div className="relative min-w-0 flex-1 overflow-y-auto px-7 py-6">
          <Button variant="ghost-muted" size="icon-xs" className="absolute right-3 top-3" aria-label={t("settings.settingsDialog.closeSettings")} onClick={onClose}>
            <X />
          </Button>
          {section === "general" && <GeneralSection />}
          {section === "editor" && <EditorSection />}
          {section === "assistant" && <AssistantSection />}
          {section === "updates" && <UpdatesSection />}
          {section === "about" && <AboutSection />}
        </div>
      </div>
    </div>,
    document.body,
  );
}
