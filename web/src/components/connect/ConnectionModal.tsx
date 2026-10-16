import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ArrowLeft, ArrowRight, Check, Loader, PlugZap, TriangleAlert, X } from "lucide-react";
import type { SavedConnection } from "@/types";
import {
  applyUrl,
  configuredTabs,
  connectionUrlOf,
  missingFields,
  newDraft,
  saveInputOf,
  savedTagOf,
  sshConfigOf,
  tabsFor,
  type ConnectionDraft,
  type ConnectionTab,
} from "../../lib/connectionDraft";
import { isSocketHost } from "../../lib/connectionUrl";
import { ENGINE_CARDS } from "../../lib/engines";
import { hostKeyProblem } from "../../lib/sshHostKey";
import { api } from "../../lib/api";
import { cn } from "../../lib/cn";
import { useConnection } from "../../contexts/useConnection";
import { useHostKeyCheck } from "../../hooks/useHostKeyCheck";
import Button from "../ui/Button";
import Badge from "../ui/Badge";
import SegmentedTabs from "../ui/SegmentedTabs";
import EnginePicker from "./EnginePicker";
import IdentityEditor from "./IdentityEditor";
import ImportUrlDialog from "./ImportUrlDialog";
import OptionsTab from "./OptionsTab";
import PostgresTabs from "./postgres/PostgresTabs";

const TAB_LABELS: Record<ConnectionTab, string> = {
  connection: "Connection",
  ssl: "SSL",
  ssh: "SSH tunnel",
  options: "Options",
};

type TestState = { status: "idle" } | { status: "testing" } | { status: "ok"; version: string } | { status: "error"; message: string };

function errorMessage(err: unknown) {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Connection failed";
}

/**
 * Creates or edits a connection: pick the database (new ones only), then its details in tabs,
 * with the connection's name, colour and icon on top. Test, save or connect from the footer.
 */
export default function ConnectionModal(props: {
  open: boolean;
  /** null starts a new connection at the database step; a draft (editing, or from a URL) opens its tabs. */
  initial: ConnectionDraft | null;
  onClose: () => void;
  onSaved: (conn: SavedConnection) => void;
  onConnected: () => void;
}) {
  // Mounted afresh each time it opens, so nothing (a test result, an error) carries over
  if (!props.open) return null;
  return <ModalBody {...props} />;
}

function ModalBody({
  initial,
  onClose,
  onSaved,
  onConnected,
}: {
  initial: ConnectionDraft | null;
  onClose: () => void;
  onSaved: (conn: SavedConnection) => void;
  onConnected: () => void;
}) {
  const isNew = initial === null || initial.id === null;
  const [step, setStep] = useState<"engine" | "form">(initial ? "form" : "engine");
  const [draft, setDraft] = useState<ConnectionDraft>(initial ?? newDraft("postgres"));
  const [tab, setTab] = useState<ConnectionTab>("connection");
  const [showErrors, setShowErrors] = useState(false);
  const [test, setTest] = useState<TestState>({ status: "idle" });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const { connect, isConnecting } = useConnection();
  const { withHostKeyCheck, prompt: hostKeyPrompt } = useHostKeyCheck();

  // Escape closes, unless something opened on top of it (a popover, a confirmation) is the one to close
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      const dialogs = document.querySelectorAll("[role=dialog]");
      if (dialogs[dialogs.length - 1] === dialogRef.current) onClose();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  function change(next: ConnectionDraft) {
    setDraft(next);
    setTest({ status: "idle" });
  }

  /** Points at the first empty field the action needs; true when there's none. */
  function ready(purpose: "connect" | "save") {
    const missing = missingFields(draft, purpose);
    if (missing.length === 0) return true;
    setShowErrors(true);
    const first = missing[0];
    if (first.tab !== "identity") setTab(first.tab);
    // After the tab with the field has rendered; looked up in the modal, whatever is behind it
    setTimeout(() => dialogRef.current?.querySelector<HTMLElement>(`#${first.fieldId}`)?.focus(), 0);
    return false;
  }

  async function handleTest() {
    if (!ready("connect")) return;
    setError(null);
    const attempt = async (trustHostKey?: string) => {
      setTest({ status: "testing" });
      try {
        const res = await api.connect(connectionUrlOf(draft), sshConfigOf(draft), trustHostKey);
        api.disconnect(res.sessionId).catch(() => {});
        setTest({ status: "ok", version: res.serverVersion.split(" ")[0] });
      } catch (err) {
        if (hostKeyProblem(err)) {
          setTest({ status: "idle" });
          throw err;
        }
        setTest({ status: "error", message: errorMessage(err) });
      }
    };
    await withHostKeyCheck(attempt).catch(() => {});
  }

  async function handleSave() {
    if (!ready("save")) return;
    setSaving(true);
    setError(null);
    try {
      const [saved] = await Promise.all([api.saveConnection(saveInputOf(draft)), new Promise((r) => setTimeout(r, 400))]);
      onSaved(saved);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setSaving(false);
    }
  }

  async function handleConnect() {
    if (!ready("connect")) return;
    setError(null);
    const attempt = async (trustHostKey?: string) => {
      try {
        await connect(connectionUrlOf(draft), savedTagOf(draft), { ssh: sshConfigOf(draft), trustHostKey });
        onConnected();
      } catch (err) {
        // The SSH server's key is asked about, not shown as an error
        if (hostKeyProblem(err)) throw err;
        setError(errorMessage(err));
      }
    };
    await withHostKeyCheck(attempt).catch(() => {});
  }

  const configured = configuredTabs(draft);
  const errorTabs = new Set(showErrors ? missingFields(draft, "connect").map((m) => m.tab) : []);
  const socket = isSocketHost(draft.fields.host);
  const engineLabel = ENGINE_CARDS.find((c) => c.id === draft.engine)?.label;
  const nameMissing = showErrors && !draft.identity.name.trim() && missingFields(draft, "save").some((m) => m.fieldId === "conn-name");

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* A click outside doesn't close it: what was typed would be lost */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px] animate-fade-in" />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={isNew ? "New connection" : "Edit connection"}
        className={cn(
          "relative flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl animate-dialog-in",
          // The tabs keep one height, so switching between them doesn't resize the window
          step === "form" && "h-[min(620px,100%)]",
        )}
      >
        <div data-connection-modal className="flex min-h-0 flex-1 flex-col">
          <div className="flex shrink-0 items-start gap-3 px-5 pt-5 pb-3">
            {step === "engine" ? (
              <div className="min-w-0 flex-1">
                <h2 className="text-[15px] font-semibold leading-tight">New connection</h2>
                <p className="mt-1 text-sm text-muted-foreground">Choose the database to connect to.</p>
              </div>
            ) : (
              <>
                <IdentityEditor layout="header" identity={draft.identity} onChange={(identity) => setDraft({ ...draft, identity })} nameInvalid={nameMissing} />
                <span className="mt-2 shrink-0 rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">{engineLabel}</span>
              </>
            )}
            <Button variant="ghost-muted" size="icon-xs" className="-mr-1.5 mt-0.5" aria-label="Close" onClick={onClose}>
              <X />
            </Button>
          </div>

          {step === "engine" ? (
            <div className="min-h-0 flex-1 overflow-y-auto px-5 pt-2 pb-5">
              <EnginePicker
                onPick={(engine) => {
                  if (engine !== "postgres") return;
                  setDraft((d) => ({ ...d, engine }));
                  setStep("form");
                }}
              />
            </div>
          ) : (
            <>
              <div className="shrink-0 border-b border-border px-5 pb-3">
                <SegmentedTabs<ConnectionTab>
                  fullWidth
                  value={tab}
                  onChange={setTab}
                  options={tabsFor(draft.engine).map((id) => {
                    const off = socket && (id === "ssl" || id === "ssh");
                    return {
                      value: id,
                      disabled: off,
                      hint: off ? "Not used with a Unix socket" : undefined,
                      label: (
                        <span className={cn("flex items-center gap-1.5", errorTabs.has(id) && "text-destructive-foreground")}>
                          {TAB_LABELS[id]}
                          {errorTabs.has(id) ? (
                            <span data-error className="size-1.5 rounded-full bg-destructive" />
                          ) : (
                            configured.has(id) && <span data-configured className="size-1.5 rounded-full bg-primary" />
                          )}
                        </span>
                      ),
                    };
                  })}
                />
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
                {tab === "options" ? (
                  <OptionsTab draft={draft} onChange={change} />
                ) : (
                  <PostgresTabs tab={tab} draft={draft} onChange={change} showErrors={showErrors} onOpenImport={() => setImportOpen(true)} />
                )}
              </div>

              {(error || test.status === "error") && (
                <div
                  data-connection-error={error ? "" : undefined}
                  className="mx-5 mb-3 flex shrink-0 items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-2.5 text-sm text-destructive-foreground"
                >
                  <TriangleAlert size={15} className="mt-0.5 shrink-0" />
                  <span className={cn("min-w-0 break-words", !error && "font-mono text-xs leading-5")}>
                    {error ?? (test.status === "error" ? test.message : null)}
                  </span>
                </div>
              )}

              <footer className="flex shrink-0 items-center gap-2 border-t border-border bg-muted px-5 py-3">
                {isNew && (
                  <Button variant="ghost-muted" size="sm" onClick={() => setStep("engine")}>
                    <ArrowLeft />
                    Back
                  </Button>
                )}
                {test.status === "ok" && (
                  <Badge variant="success">
                    <Check />
                    OK · {engineLabel} {test.version}
                  </Badge>
                )}
                {test.status === "error" && (
                  <Badge variant="destructive">
                    <X />
                    Connection failed
                  </Badge>
                )}
                <div className="flex-1" />
                <Button variant="outline" size="sm" onClick={handleTest} disabled={test.status === "testing"}>
                  {test.status === "testing" ? <Loader className="animate-spin" /> : <PlugZap />}
                  Test
                </Button>
                <Button variant="outline" size="sm" onClick={handleSave} disabled={saving}>
                  {saving && <Loader className="animate-spin" />}
                  Save
                </Button>
                <Button variant="primary" size="sm" className="group" onClick={handleConnect} disabled={isConnecting}>
                  {isConnecting ? (
                    <>
                      <Loader className="animate-spin" />
                      Connecting…
                    </>
                  ) : (
                    <>
                      Connect
                      <ArrowRight className="transition-transform group-hover:translate-x-0.5" />
                    </>
                  )}
                </Button>
              </footer>
            </>
          )}
        </div>
      </div>

      {hostKeyPrompt}
      <ImportUrlDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImport={(url) => {
          const filled = applyUrl(draft, url);
          if (!filled) return false;
          change(filled);
          setImportOpen(false);
          return true;
        }}
      />
    </div>,
    document.body,
  );
}
