import { useState, useEffect, useRef, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { getVersion } from "@tauri-apps/api/app";
import { ArrowRight, Check, Database, Eye, EyeOff, FolderOpen, KeyRound, Link, Loader, PlugZap, ShieldAlert, TriangleAlert, X } from "lucide-react";
import { useConnection } from "../contexts/useConnection";
import { useWorkspace } from "../contexts/workspace";
import { type ConnectionFields, parseConnectionUrl, buildConnectionUrl, tlsFieldsOf, tlsSettingsOf } from "../lib/connectionUrl";
import { api } from "../lib/api";
import { cn } from "../lib/cn";
import type { SavedConnection, SshAuthMethod, SshConfig, SshHostKeyProblem, SslMode } from "@/types";
import { hostKeyProblem } from "../lib/sshHostKey";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import ConnectionSidebar from "../components/connect/ConnectionSidebar";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { IdleStatusBar } from "../components/layout/StatusBar";
import Button from "../components/ui/Button";
import Input from "../components/ui/Input";
import Field from "../components/ui/Field";
import Switch from "../components/ui/Switch";
import Badge from "../components/ui/Badge";
import Dialog from "../components/ui/Dialog";
import Popover from "../components/ui/Popover";
import ColorPicker from "../components/ui/ColorPicker";

const DEFAULT_COLOR = "#3b82f6";
// Colour tags to tell environments apart (prod, staging, local...)
const COLOR_PRESETS = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899"];

/** The SSH form keeps the port as typed; it becomes a number when used. */
type SshFields = Omit<SshConfig, "port"> & { port: string };
const EMPTY_SSH: SshFields = { host: "", port: "", user: "", authMethod: "password", password: "", keyPath: "", passphrase: "" };

function toSshFields(ssh: SshConfig): SshFields {
  return { ...ssh, port: ssh.port && ssh.port !== 22 ? String(ssh.port) : "" };
}

const EMPTY_FIELDS: ConnectionFields = {
  host: "",
  port: "",
  database: "",
  user: "",
  password: "",
  ssl: false,
  sslMode: "require",
  sslRootCert: "",
  sslCert: "",
  sslKey: "",
  readOnly: false,
};

type TestState =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "ok"; version: string }
  | { status: "error"; message: string };

function errorMessage(err: unknown) {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Connection failed";
}

function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-xl border border-border bg-card shadow-xs/5">
      <div className={cn("flex items-center gap-3 px-4 py-3", children && "border-b border-border")}>
        <div className="min-w-0 flex-1">
          <h2 className="text-[13px] font-semibold">{title}</h2>
          {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
        </div>
        {action}
      </div>
      {/* A collapsed section (e.g. SSH turned off) is just its header */}
      {children && <div className="flex flex-col gap-4 p-4">{children}</div>}
    </section>
  );
}

function HostKeyFingerprint({ problem }: { problem: SshHostKeyProblem }) {
  return (
    <div className="rounded-lg border border-border bg-muted px-3 py-2.5">
      <div className="text-xs text-muted-foreground">
        {problem.host}:{problem.port} · {problem.algorithm}
      </div>
      <code className="mt-1 block break-all font-mono text-xs">{problem.fingerprint}</code>
    </div>
  );
}

export default function ConnectPage() {
  const [fields, setFields] = useState<ConnectionFields>(EMPTY_FIELDS);
  const [connectionName, setConnectionName] = useState("");
  const [connectionColor, setConnectionColor] = useState(DEFAULT_COLOR);
  const [showImport, setShowImport] = useState(false);
  const [importUrl, setImportUrl] = useState("");
  const [importError, setImportError] = useState(false);
  const [savedConnections, setSavedConnections] = useState<SavedConnection[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [test, setTest] = useState<TestState>({ status: "idle" });
  const [version, setVersion] = useState<string | null>(null);
  const [colorPickerOpen, setColorPickerOpen] = useState(false);
  const customColorRef = useRef<HTMLButtonElement>(null);
  const [sshEnabled, setSshEnabled] = useState(false);
  const [ssh, setSsh] = useState<SshFields>(EMPTY_SSH);
  const [showSshSecret, setShowSshSecret] = useState(false);
  // An SSH server key waiting for the user's decision, and how to retry once trusted
  const [hostKeyPrompt, setHostKeyPrompt] = useState<{ problem: SshHostKeyProblem; retry: () => void } | null>(null);

  const { connect, isConnecting, error } = useConnection();
  const { sidebarOpen } = useWorkspace();
  const navigate = useNavigate();

  useEffect(() => {
    loadSavedConnections();
    getVersion().then(setVersion).catch(() => {});
  }, []);

  async function loadSavedConnections() {
    try {
      setSavedConnections(await api.getSavedConnections());
    } catch {
      setSavedConnections([]);
    }
  }

  function updateSsh<K extends keyof SshFields>(key: K, value: SshFields[K]) {
    setSsh((prev) => ({ ...prev, [key]: value }));
    setTest({ status: "idle" });
  }

  /** The SSH settings to connect with, or null for a direct connection. */
  function sshConfig(): SshConfig | null {
    if (!sshEnabled) return null;
    return { ...ssh, host: ssh.host.trim(), user: ssh.user.trim(), port: parseInt(ssh.port || "22", 10) };
  }

  /**
   * Runs a connection attempt; if the SSH server's key needs the user's say, asks, and
   * runs it again with the fingerprint they accepted.
   */
  async function withHostKeyCheck(attempt: (trustHostKey?: string) => Promise<void>) {
    try {
      await attempt();
    } catch (err) {
      const problem = hostKeyProblem(err);
      if (!problem) throw err;
      setHostKeyPrompt({
        problem,
        retry: () => {
          setHostKeyPrompt(null);
          attempt(problem.fingerprint).catch(() => {});
        },
      });
    }
  }

  async function browseCertificate(key: "sslRootCert" | "sslCert" | "sslKey") {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, title: "Choose a certificate or key file" });
    if (typeof path === "string") updateField(key, path);
  }

  async function browseKey() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, title: "Choose a private key" });
    if (typeof path === "string") updateSsh("keyPath", path);
  }

  function updateField(key: keyof ConnectionFields, value: string | boolean) {
    setFields((prev) => ({ ...prev, [key]: value }));
    setTest({ status: "idle" });
  }

  /** Fills the form from a postgres:// URL. Returns false if it isn't one. */
  function importFromUrl(url: string) {
    const parsed = parseConnectionUrl(url.trim());
    if (!/^postgres(ql)?:\/\//i.test(url.trim()) || !parsed.host) return false;
    setFields((prev) => ({ ...prev, ...parsed }));
    setTest({ status: "idle" });
    return true;
  }

  function handleImport() {
    if (!importFromUrl(importUrl)) {
      setImportError(true);
      return;
    }
    closeImport();
  }

  function closeImport() {
    setShowImport(false);
    setImportUrl("");
    setImportError(false);
  }

  function handleSelectSaved(conn: SavedConnection) {
    setSelectedId(conn.id);
    setConnectionName(conn.name);
    setConnectionColor(conn.color || DEFAULT_COLOR);
    setFields({
      host: conn.host,
      port: conn.port.toString(),
      database: conn.database,
      user: conn.user,
      password: conn.password,
      ssl: conn.ssl,
      readOnly: conn.readOnly ?? false,
      ...tlsFieldsOf(conn.tls),
    });
    setSshEnabled(Boolean(conn.ssh));
    setSsh(conn.ssh ? toSshFields(conn.ssh) : EMPTY_SSH);
    setTest({ status: "idle" });
  }

  async function handleConnectSaved(conn: SavedConnection) {
    handleSelectSaved(conn);
    try {
      const url = buildConnectionUrl({
        host: conn.host,
        port: conn.port.toString(),
        database: conn.database,
        user: conn.user,
        password: conn.password,
        ssl: conn.ssl,
        readOnly: conn.readOnly ?? false,
        ...tlsFieldsOf(conn.tls),
      });
      await withHostKeyCheck(async (trustHostKey) => {
        await connect(url, { name: conn.name, color: conn.color }, { ssh: conn.ssh, trustHostKey });
        navigate("/db/query");
      });
    } catch {
      // the error is shown by the connection context
    }
  }

  function handleNewConnection() {
    setSelectedId(null);
    setConnectionName("");
    setConnectionColor(DEFAULT_COLOR);
    setFields(EMPTY_FIELDS);
    setSshEnabled(false);
    setSsh(EMPTY_SSH);
    setTest({ status: "idle" });
  }

  async function handleSave() {
    if (!connectionName.trim() || !fields.host.trim() || !fields.database.trim()) return;

    setIsSaving(true);
    try {
      const [saved] = await Promise.all([
        api.saveConnection({
          id: selectedId || undefined,
          name: connectionName.trim(),
          host: fields.host.trim(),
          port: parseInt(fields.port || "5432", 10),
          database: fields.database.trim(),
          user: fields.user.trim(),
          password: fields.password,
          ssl: fields.ssl,
          readOnly: fields.readOnly,
          tls: tlsSettingsOf(fields),
          ssh: sshConfig(),
          color: connectionColor,
        }),
        new Promise((r) => setTimeout(r, 400)),
      ]);
      // Keep the saved item selected, so saving again updates instead of duplicating
      setSelectedId(saved.id);
      await loadSavedConnections();
    } catch {
      // keep the form as is; nothing was saved
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.deleteSavedConnection(id);
      if (selectedId === id) handleNewConnection();
      await loadSavedConnections();
    } catch {
      // list is reloaded on the next action
    }
  }

  /** Opens and closes a session to check the details, without leaving the form. */
  async function handleTest() {
    const attempt = async (trustHostKey?: string) => {
      setTest({ status: "testing" });
      try {
        const res = await api.connect(buildConnectionUrl(fields), sshConfig(), trustHostKey);
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

  async function handleSubmit(e: { preventDefault: () => void }) {
    e.preventDefault();
    try {
      const savedName = connectionName.trim();
      await withHostKeyCheck(async (trustHostKey) => {
        await connect(
          buildConnectionUrl(fields),
          savedName ? { name: savedName, color: connectionColor } : undefined,
          { ssh: sshConfig(), trustHostKey },
        );
        navigate("/db/query");
      });
    } catch {
      // the error is shown by the connection context
    }
  }

  const sshReady =
    !sshEnabled ||
    Boolean(ssh.host.trim() && ssh.user.trim() && (ssh.authMethod !== "key" || ssh.keyPath.trim()));
  const canConnect = Boolean(fields.host.trim() && fields.database.trim()) && sshReady;
  const canSave = canConnect && Boolean(connectionName.trim());
  const editing = selectedId !== null;
  const isCustomColor = !COLOR_PRESETS.includes(connectionColor);

  return (
    <div className="h-full flex flex-col">
      <div className="flex flex-1 min-h-0 gap-1.5">
        <ConnectionSidebar
          connections={savedConnections}
          selectedId={selectedId}
          expanded={sidebarOpen}
          onSelect={handleSelectSaved}
          onConnect={handleConnectSaved}
          onDelete={handleDelete}
          onNew={handleNewConnection}
        />

        <main className="flex min-w-0 flex-1 flex-col rounded-lg border border-border bg-card overflow-hidden">
          <PageHeader>
            <HeaderTitle label={editing ? "Saved connection" : "Connection"} icon={<Database />}>
              {editing ? connectionName || "Unnamed" : "New connection"}
            </HeaderTitle>
          </PageHeader>

          <div className="flex-1 overflow-y-auto">
            <form onSubmit={handleSubmit} className="mx-auto flex w-full max-w-xl flex-col gap-5 px-6 py-8">
              <div className="flex items-center gap-3 select-none">
                <img src="/icon.svg" className="size-10 invert dark:invert-0" alt="" />
                <div>
                  <h1 className="text-lg font-semibold leading-tight">
                    {editing ? "Edit connection" : "Connect to PostgreSQL"}
                  </h1>
                  <p className="mt-0.5 text-sm text-muted-foreground">
                    {editing ? "Update the details, then save or connect." : "Enter the server details, or paste a connection URL into Host."}
                  </p>
                </div>
              </div>

              <Section
                title="Server"
                action={
                  <Button variant="ghost-muted" size="xs" onClick={() => setShowImport(true)}>
                    <Link />
                    Import from URL
                  </Button>
                }
              >
                <div className="grid grid-cols-[1fr_6.5rem] gap-3">
                  <Field label="Host" htmlFor="conn-host">
                    <Input
                      id="conn-host"
                      autoFocus
                      value={fields.host}
                      onChange={(e) => updateField("host", e.target.value)}
                      onPaste={(e) => {
                        if (importFromUrl(e.clipboardData.getData("text"))) e.preventDefault();
                      }}
                      placeholder="localhost"
                      spellCheck={false}
                    />
                  </Field>
                  <Field label="Port" htmlFor="conn-port">
                    <Input
                      id="conn-port"
                      inputMode="numeric"
                      value={fields.port}
                      onChange={(e) => updateField("port", e.target.value)}
                      placeholder="5432"
                      className="font-mono"
                    />
                  </Field>
                </div>
                <Field label="Database" htmlFor="conn-db">
                  <Input
                    id="conn-db"
                    value={fields.database}
                    onChange={(e) => updateField("database", e.target.value)}
                    placeholder="my_database"
                    spellCheck={false}
                  />
                </Field>
                <div className="grid grid-cols-2 gap-3">
                  <Field label="User" htmlFor="conn-user">
                    <Input
                      id="conn-user"
                      value={fields.user}
                      onChange={(e) => updateField("user", e.target.value)}
                      placeholder="postgres"
                      spellCheck={false}
                      autoComplete="off"
                    />
                  </Field>
                  <Field label="Password" htmlFor="conn-password">
                    <div className="relative">
                      <Input
                        id="conn-password"
                        type={showPassword ? "text" : "password"}
                        value={fields.password}
                        onChange={(e) => updateField("password", e.target.value)}
                        placeholder="••••••••"
                        className="pr-8"
                        autoComplete="off"
                      />
                      <Button
                        variant="ghost-muted"
                        size="icon-xs"
                        className="absolute right-1 top-1/2 -translate-y-1/2"
                        aria-label={showPassword ? "Hide password" : "Show password"}
                        onClick={() => setShowPassword((v) => !v)}
                      >
                        {showPassword ? <EyeOff /> : <Eye />}
                      </Button>
                    </div>
                  </Field>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <label htmlFor="conn-ssl" className="select-none">
                    <span className="block text-[13px] font-medium">Require SSL</span>
                    <span className="block text-xs text-muted-foreground">Encrypt the connection</span>
                  </label>
                  <Switch id="conn-ssl" checked={fields.ssl} onChange={(v) => updateField("ssl", v)} />
                </div>
                {fields.ssl && (
                  <div className="flex flex-col gap-3 rounded-lg border border-border bg-muted/50 p-3">
                    <div className="flex flex-col gap-1.5">
                      <span className="text-[13px] font-medium">Check the server</span>
                      <SegmentedTabs<SslMode>
                        value={fields.sslMode}
                        onChange={(v) => updateField("sslMode", v)}
                        options={[
                          { value: "require", label: "Encrypt only" },
                          { value: "verify-ca", label: "Certificate" },
                          { value: "verify-full", label: "Certificate + name" },
                        ]}
                      />
                      <span className="text-xs text-muted-foreground">
                        {fields.sslMode === "require"
                          ? fields.sslRootCert.trim()
                            ? "With a CA certificate the server's certificate is checked against it."
                            : "The server's certificate isn't checked (common for managed databases)."
                          : fields.sslMode === "verify-ca"
                            ? "The server's certificate must be signed by a trusted CA."
                            : "The certificate must be trusted and issued for this host name."}
                      </span>
                    </div>
                    {(
                      [
                        ["sslRootCert", "CA certificate", "Optional: the CA that signed the server's certificate"],
                        ["sslCert", "Client certificate", "Optional: for servers that log in by certificate"],
                        ["sslKey", "Client key", "Unencrypted PKCS#8 PEM"],
                      ] as const
                    ).map(([key, label, hint]) => (
                      <Field key={key} label={label} htmlFor={`conn-${key}`}>
                        <div className="flex gap-2">
                          <Input
                            id={`conn-${key}`}
                            value={fields[key]}
                            onChange={(e) => updateField(key, e.target.value)}
                            placeholder={hint}
                            spellCheck={false}
                            className="font-mono text-xs"
                          />
                          <Button variant="outline" onClick={() => browseCertificate(key)} aria-label={`Choose the ${label.toLowerCase()} file`}>
                            <FolderOpen />
                          </Button>
                        </div>
                      </Field>
                    ))}
                  </div>
                )}
                <div className="flex items-center justify-between gap-4">
                  <label htmlFor="conn-readonly" className="select-none">
                    <span className="block text-[13px] font-medium">Read-only</span>
                    <span className="block text-xs text-muted-foreground">The server refuses inserts, updates, deletes and schema changes</span>
                  </label>
                  <Switch id="conn-readonly" checked={fields.readOnly} onChange={(v) => updateField("readOnly", v)} />
                </div>
              </Section>

              <Section
                title="SSH tunnel"
                description={
                  sshEnabled
                    ? "Host and port above are the database as seen from the SSH server."
                    : "Reach the database through an SSH server."
                }
                action={<Switch id="conn-ssh" checked={sshEnabled} onChange={(v) => { setSshEnabled(v); setTest({ status: "idle" }); }} aria-label="Connect through SSH" />}
              >
                {sshEnabled && (
                  <>
                    <div className="grid grid-cols-[1fr_6.5rem] gap-3">
                      <Field label="SSH host" htmlFor="ssh-host">
                        <Input id="ssh-host" value={ssh.host} onChange={(e) => updateSsh("host", e.target.value)} placeholder="bastion.example.com" spellCheck={false} />
                      </Field>
                      <Field label="Port" htmlFor="ssh-port">
                        <Input id="ssh-port" inputMode="numeric" value={ssh.port} onChange={(e) => updateSsh("port", e.target.value)} placeholder="22" className="font-mono" />
                      </Field>
                    </div>
                    <Field label="SSH user" htmlFor="ssh-user">
                      <Input id="ssh-user" value={ssh.user} onChange={(e) => updateSsh("user", e.target.value)} placeholder="ubuntu" spellCheck={false} autoComplete="off" />
                    </Field>
                    <div className="flex flex-col gap-1.5">
                      <span className="text-[13px] font-medium">Log in with</span>
                      <SegmentedTabs<SshAuthMethod>
                        value={ssh.authMethod}
                        onChange={(v) => updateSsh("authMethod", v)}
                        options={[
                          { value: "password", label: "Password" },
                          { value: "key", label: "Private key" },
                          { value: "agent", label: "SSH agent" },
                        ]}
                      />
                    </div>
                    {ssh.authMethod === "password" && (
                      <Field label="SSH password" htmlFor="ssh-password">
                        <div className="relative">
                          <Input
                            id="ssh-password"
                            type={showSshSecret ? "text" : "password"}
                            value={ssh.password}
                            onChange={(e) => updateSsh("password", e.target.value)}
                            className="pr-9"
                            autoComplete="off"
                          />
                          <Button variant="ghost-muted" size="icon-xs" className="absolute right-1 top-1/2 -translate-y-1/2" aria-label={showSshSecret ? "Hide password" : "Show password"} onClick={() => setShowSshSecret((v) => !v)}>
                            {showSshSecret ? <EyeOff /> : <Eye />}
                          </Button>
                        </div>
                      </Field>
                    )}
                    {ssh.authMethod === "key" && (
                      <>
                        <Field label="Private key" htmlFor="ssh-key">
                          <div className="flex gap-2">
                            <Input id="ssh-key" value={ssh.keyPath} onChange={(e) => updateSsh("keyPath", e.target.value)} placeholder="~/.ssh/id_ed25519" spellCheck={false} className="font-mono text-xs" />
                            <Button variant="outline" onClick={browseKey}>
                              <FolderOpen />
                              Browse
                            </Button>
                          </div>
                        </Field>
                        <Field label="Passphrase" htmlFor="ssh-passphrase">
                          <Input id="ssh-passphrase" type="password" value={ssh.passphrase} onChange={(e) => updateSsh("passphrase", e.target.value)} placeholder="Only if the key is encrypted" autoComplete="off" />
                        </Field>
                      </>
                    )}
                    {ssh.authMethod === "agent" && (
                      <p className="flex items-center gap-2 text-xs text-muted-foreground">
                        <KeyRound size={13} className="shrink-0" />
                        Uses the keys loaded in your running SSH agent (ssh-add).
                      </p>
                    )}
                  </>
                )}
              </Section>

              <Section title="Save as" description="Optional. Keeps the connection in the sidebar for one-click access.">
                <Field label="Name" htmlFor="conn-name">
                  <Input
                    id="conn-name"
                    value={connectionName}
                    onChange={(e) => setConnectionName(e.target.value)}
                    placeholder="e.g. Production"
                    spellCheck={false}
                  />
                </Field>
                <Field label="Color" hint="Tag environments, e.g. red for production.">
                  <div className="flex items-center gap-2">
                    {COLOR_PRESETS.map((color) => (
                      <button
                        key={color}
                        type="button"
                        aria-label={`Color ${color}`}
                        aria-pressed={connectionColor === color}
                        onClick={() => setConnectionColor(color)}
                        className={cn(
                          "size-5 rounded-full cursor-pointer outline-none transition-shadow focus-visible:ring-2 focus-visible:ring-ring/50",
                          connectionColor === color && "ring-2 ring-offset-2 ring-offset-card",
                        )}
                        style={{ backgroundColor: color, ["--tw-ring-color" as string]: color }}
                      />
                    ))}
                    <span className="mx-0.5 h-4 w-px bg-border" />
                    <button
                      ref={customColorRef}
                      type="button"
                      aria-label="Custom color"
                      aria-expanded={colorPickerOpen}
                      onClick={() => setColorPickerOpen((v) => !v)}
                      className={cn(
                        "size-5 rounded-full cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                        (isCustomColor || colorPickerOpen) && "ring-2 ring-offset-2 ring-offset-card",
                      )}
                      style={{
                        background: isCustomColor
                          ? connectionColor
                          : "conic-gradient(#ef4444, #eab308, #22c55e, #3b82f6, #8b5cf6, #ef4444)",
                        ["--tw-ring-color" as string]: isCustomColor ? connectionColor : "var(--ring)",
                      }}
                    />
                    <Popover open={colorPickerOpen} onClose={() => setColorPickerOpen(false)} anchorRef={customColorRef} className="p-3">
                      <ColorPicker value={connectionColor} onChange={setConnectionColor} />
                    </Popover>
                  </div>
                </Field>
              </Section>

              {error && (
                <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-2.5 text-sm text-destructive-foreground">
                  <TriangleAlert size={15} className="mt-0.5 shrink-0" />
                  <span className="min-w-0 break-words">{error}</span>
                </div>
              )}

              <div className="flex items-center gap-2">
                <Button variant="outline" onClick={handleTest} disabled={!canConnect || test.status === "testing"}>
                  {test.status === "testing" ? <Loader className="animate-spin" /> : <PlugZap />}
                  Test
                </Button>
                {test.status === "ok" && (
                  <Badge variant="success">
                    <Check />
                    OK · PostgreSQL {test.version}
                  </Badge>
                )}
                {test.status === "error" && (
                  <Badge variant="destructive">
                    <X />
                    Connection failed
                  </Badge>
                )}
                <div className="flex-1" />
                <Button variant="outline" onClick={handleSave} disabled={!canSave || isSaving}>
                  {isSaving && <Loader className="animate-spin" />}
                  {editing ? "Update" : "Save"}
                </Button>
                <Button type="submit" variant="primary" className="group px-4" disabled={!canConnect || isConnecting}>
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
              </div>
              {test.status === "error" && (
                <p className="-mt-2 break-words font-mono text-xs text-destructive-foreground">{test.message}</p>
              )}
            </form>
          </div>
        </main>
      </div>

      <IdleStatusBar version={version} />

      {/* The SSH server's key: first time, or different from before */}
      {hostKeyPrompt && hostKeyPrompt.problem.inUserKnownHosts ? (
        <Dialog
          open
          onClose={() => setHostKeyPrompt(null)}
          icon={<ShieldAlert />}
          tone="destructive"
          title="The SSH server's key changed"
          description={`${hostKeyPrompt.problem.host} presented a different key than the one in ~/.ssh/known_hosts. Someone may be intercepting the connection. If the server was reinstalled, remove its old entry from known_hosts and try again.`}
          footer={
            <Button variant="neutral" size="sm" onClick={() => setHostKeyPrompt(null)}>
              Close
            </Button>
          }
        >
          <HostKeyFingerprint problem={hostKeyPrompt.problem} />
        </Dialog>
      ) : (
        <ConfirmDialog
          open={hostKeyPrompt !== null}
          onClose={() => setHostKeyPrompt(null)}
          onConfirm={() => hostKeyPrompt?.retry()}
          icon={<ShieldAlert />}
          tone={hostKeyPrompt?.problem.status === "changed" ? "destructive" : "warning"}
          title={hostKeyPrompt?.problem.status === "changed" ? "The SSH server's key changed" : "Trust this SSH server?"}
          description={
            hostKeyPrompt?.problem.status === "changed"
              ? "It's different from the key accepted before. Someone may be intercepting the connection; trust the new key only if you know the server changed."
              : "This is the first connection to this server. Check that the fingerprint matches the one your server administrator gives you."
          }
          cancelLabel="Cancel"
          confirmLabel={hostKeyPrompt?.problem.status === "changed" ? "Trust the new key" : "Trust and connect"}
        >
          {hostKeyPrompt && <HostKeyFingerprint problem={hostKeyPrompt.problem} />}
        </ConfirmDialog>
      )}

      <Dialog
        open={showImport}
        onClose={closeImport}
        title="Import from URL"
        description="Paste a PostgreSQL connection string to fill in the form."
        footer={
          <>
            <Button variant="neutral" size="sm" onClick={closeImport}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={handleImport} disabled={!importUrl.trim()}>
              Import
            </Button>
          </>
        }
      >
        <Input
          autoFocus
          value={importUrl}
          onChange={(e) => {
            setImportUrl(e.target.value);
            setImportError(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              handleImport();
            }
          }}
          placeholder="postgresql://user:password@host:5432/database"
          spellCheck={false}
          className="font-mono text-xs"
          aria-invalid={importError}
        />
        {importError && (
          <p className="mt-2 text-xs text-destructive-foreground">That doesn't look like a postgres:// connection URL.</p>
        )}
      </Dialog>
    </div>
  );
}
