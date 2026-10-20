import { useState, type ReactNode } from "react";
import { Eye, EyeOff, FolderOpen, KeyRound, Link } from "lucide-react";
import type { SshAuthMethod, SslMode } from "@/types";
import { applyUrl, missingFields, type ConnectionDraft, type ConnectionTab, type SshFields } from "../../../lib/connectionDraft";
import type { ConnectionFields } from "../../../lib/connectionUrl";
import { isSocketHost } from "../../../lib/connectionUrl";
import { isMysqlFamily } from "../../../lib/engines";
import { isWindows } from "../../../lib/platform";
import SegmentedTabs from "../../ui/SegmentedTabs";
import Button from "../../ui/Button";
import Input from "../../ui/Input";
import Field from "../../ui/Field";
import Switch from "../../ui/Switch";
import { useT } from "../../../i18n";

/** A switch with its label and what it does, side by side. */
function SwitchRow({ id, title, hint, checked, onChange }: { id: string; title: string; hint: ReactNode; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <label htmlFor={id} className="select-none">
        <span className="block text-[13px] font-medium">{title}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </label>
      <Switch id={id} checked={checked} onChange={onChange} />
    </div>
  );
}

function PasswordInput({ id, value, onChange, placeholder, invalid }: { id: string; value: string; onChange: (v: string) => void; placeholder?: string; invalid?: boolean }) {
  const t = useT();
  const [shown, setShown] = useState(false);
  return (
    <div className="relative">
      <Input
        id={id}
        type={shown ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="pr-8"
        autoComplete="off"
        aria-invalid={invalid || undefined}
      />
      <Button
        variant="ghost-muted"
        size="icon-xs"
        className="absolute right-1 top-1/2 -translate-y-1/2"
        aria-label={shown ? t("connect.serverTabs.hidePassword") : t("connect.serverTabs.showPassword")}
        onClick={() => setShown((v) => !v)}
      >
        {shown ? <EyeOff /> : <Eye />}
      </Button>
    </div>
  );
}

async function pickFile(title: string) {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ multiple: false, title });
  return typeof path === "string" ? path : null;
}

/** The body of one tab of a server connection (PostgreSQL, MySQL or MariaDB): server, SSL, SSH tunnel. */
/** Account names that servers ship with, shown as examples. */
const DEFAULT_USER = { mysql: "root", postgres: "postgres" };
const LOCALHOST = "localhost";

export default function ServerTabs({
  tab,
  draft,
  onChange,
  showErrors,
  onOpenImport,
}: {
  tab: ConnectionTab;
  draft: ConnectionDraft;
  onChange: (draft: ConnectionDraft) => void;
  showErrors: boolean;
  /** Opens the import-from-URL dialog. */
  onOpenImport: () => void;
}) {
  const t = useT();
  const { fields, ssh } = draft;
  const mysql = isMysqlFamily(draft.engine);
  const socket = isSocketHost(fields.host);
  const missing = showErrors ? new Set(missingFields(draft, "connect").map((m) => m.fieldId)) : new Set<string>();
  const invalid = (id: string) => missing.has(id) || undefined;
  const setField = <K extends keyof ConnectionFields>(key: K, value: ConnectionFields[K]) => onChange({ ...draft, fields: { ...fields, [key]: value } });
  const setSsh = <K extends keyof SshFields>(key: K, value: SshFields[K]) => onChange({ ...draft, ssh: { ...ssh, [key]: value } });

  if (tab === "connection") {
    return (
      <div className="flex flex-col gap-4">
        <div className="flex items-center justify-between gap-3">
          <p className="text-xs text-muted-foreground">{t("connect.serverTabs.serverHint")}</p>
          <Button variant="ghost-muted" size="xs" onClick={onOpenImport}>
            <Link />
            {t("connect.serverTabs.importFromUrl")}
          </Button>
        </div>
        <div className="grid grid-cols-[1fr_6.5rem] gap-3">
          <Field label={t("connect.serverTabs.host")} htmlFor="conn-host">
            <Input
              id="conn-host"
              autoFocus
              value={fields.host}
              onChange={(e) => setField("host", e.target.value)}
              onPaste={(e) => {
                const filled = applyUrl(draft, e.clipboardData.getData("text"));
                if (filled) {
                  e.preventDefault();
                  onChange(filled);
                }
              }}
              placeholder={isWindows ? LOCALHOST : t("connect.serverTabs.hostPlaceholder")}
              spellCheck={false}
              aria-invalid={invalid("conn-host")}
            />
          </Field>
          <Field label={t("connect.serverTabs.port")} htmlFor="conn-port">
            <Input id="conn-port" inputMode="numeric" value={fields.port} onChange={(e) => setField("port", e.target.value)} placeholder={mysql ? "3306" : "5432"} className="font-mono" />
          </Field>
        </div>
        <Field label={mysql ? t("connect.serverTabs.databaseOptional") : t("connect.serverTabs.database")} htmlFor="conn-db">
          <Input
            id="conn-db"
            value={fields.database}
            onChange={(e) => setField("database", e.target.value)}
            placeholder={mysql ? t("connect.serverTabs.databaseEmptyHint") : t("connect.serverTabs.databasePlaceholder")}
            spellCheck={false}
            aria-invalid={invalid("conn-db")}
          />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("connect.serverTabs.user")} htmlFor="conn-user">
            <Input id="conn-user" value={fields.user} onChange={(e) => setField("user", e.target.value)} placeholder={mysql ? DEFAULT_USER.mysql : DEFAULT_USER.postgres} spellCheck={false} autoComplete="off" />
          </Field>
          <Field label={t("connect.serverTabs.password")} htmlFor="conn-password">
            <PasswordInput id="conn-password" value={fields.password} onChange={(v) => setField("password", v)} placeholder="••••••••" />
          </Field>
        </div>
        {socket && (
          <p className="-mt-1 text-xs text-muted-foreground">
            {t("connect.serverTabs.socketNote")}
          </p>
        )}
      </div>
    );
  }

  if (tab === "ssl") {
    return (
      <div className="flex flex-col gap-4">
        <SwitchRow id="conn-ssl" title={t("connect.serverTabs.requireSsl")} hint={t("connect.serverTabs.encryptTheConnection")} checked={fields.ssl} onChange={(v) => setField("ssl", v)} />
        {fields.ssl && (
          <div className="flex flex-col gap-3 rounded-lg border border-border bg-muted/50 p-3">
            <div className="flex flex-col gap-1.5">
              <span className="text-[13px] font-medium">{t("connect.serverTabs.checkTheServer")}</span>
              <SegmentedTabs<SslMode>
                value={fields.sslMode}
                onChange={(v) => setField("sslMode", v)}
                options={
                  mysql
                    ? [
                        { value: "require", label: t("connect.serverTabs.sslRequired") },
                        { value: "verify-ca", label: t("connect.serverTabs.sslVerifyCa") },
                        { value: "verify-full", label: t("connect.serverTabs.sslVerifyIdentity") },
                      ]
                    : [
                        { value: "require", label: t("connect.serverTabs.sslEncryptOnly") },
                        { value: "verify-ca", label: t("connect.serverTabs.sslCertificate") },
                        { value: "verify-full", label: t("connect.serverTabs.sslCertificateName") },
                      ]
                }
              />
              <span className="text-xs text-muted-foreground">
                {fields.sslMode === "require"
                  ? fields.sslRootCert.trim()
                    ? t("connect.serverTabs.sslCaNote")
                    : t("connect.serverTabs.sslNoCheckNote")
                  : fields.sslMode === "verify-ca"
                    ? mysql
                      ? t("connect.serverTabs.sslVerifyCaMysqlNote")
                      : t("connect.serverTabs.sslVerifyCaNote")
                    : t("connect.serverTabs.sslVerifyFullNote")}
              </span>
            </div>
            {(
              [
                ["sslRootCert", t("connect.serverTabs.caCertificate"), t("connect.serverTabs.caCertificateHint")],
                ["sslCert", t("connect.serverTabs.clientCertificate"), t("connect.serverTabs.clientCertificateHint")],
                ["sslKey", t("connect.serverTabs.clientKey"), mysql ? t("connect.serverTabs.clientKeyHintMysql") : t("connect.serverTabs.clientKeyHint")],
              ] as const
            ).map(([key, label, hint]) => (
              <Field key={key} label={label} htmlFor={`conn-${key}`}>
                <div className="flex gap-2">
                  <Input
                    id={`conn-${key}`}
                    value={fields[key]}
                    onChange={(e) => setField(key, e.target.value)}
                    placeholder={hint}
                    spellCheck={false}
                    className="font-mono text-xs"
                  />
                  <Button
                    variant="outline"
                    onClick={async () => {
                      const path = await pickFile("Choose a certificate or key file");
                      if (path) setField(key, path);
                    }}
                    aria-label={t("connect.serverTabs.chooseFile", { label })}
                  >
                    <FolderOpen />
                  </Button>
                </div>
              </Field>
            ))}
          </div>
        )}
      </div>
    );
  }

  if (tab === "ssh") {
    return (
      <div className="flex flex-col gap-4">
        <SwitchRow
          id="conn-ssh"
          title={t("connect.serverTabs.connectThroughSsh")}
          hint={draft.sshEnabled ? t("connect.serverTabs.sshOnHint") : t("connect.serverTabs.sshOffHint")}
          checked={draft.sshEnabled}
          onChange={(v) => onChange({ ...draft, sshEnabled: v })}
        />
        {draft.sshEnabled && (
          <>
            <div className="grid grid-cols-[1fr_6.5rem] gap-3">
              <Field label={t("connect.serverTabs.sshHost")} htmlFor="ssh-host">
                <Input id="ssh-host" value={ssh.host} onChange={(e) => setSsh("host", e.target.value)} placeholder={t("connect.serverTabs.sshHostPlaceholder")} spellCheck={false} aria-invalid={invalid("ssh-host")} />
              </Field>
              <Field label={t("connect.serverTabs.port")} htmlFor="ssh-port">
                <Input id="ssh-port" inputMode="numeric" value={ssh.port} onChange={(e) => setSsh("port", e.target.value)} placeholder="22" className="font-mono" />
              </Field>
            </div>
            <Field label={t("connect.serverTabs.sshUser")} htmlFor="ssh-user">
              <Input id="ssh-user" value={ssh.user} onChange={(e) => setSsh("user", e.target.value)} placeholder={t("connect.serverTabs.sshUserPlaceholder")} spellCheck={false} autoComplete="off" aria-invalid={invalid("ssh-user")} />
            </Field>
            <div className="flex flex-col gap-1.5">
              <span className="text-[13px] font-medium">{t("connect.serverTabs.logInWith")}</span>
              <SegmentedTabs<SshAuthMethod>
                value={ssh.authMethod}
                onChange={(v) => setSsh("authMethod", v)}
                options={[
                  { value: "password", label: t("connect.serverTabs.password") },
                  { value: "key", label: t("connect.serverTabs.privateKey") },
                  { value: "agent", label: t("connect.serverTabs.sshAgent") },
                ]}
              />
            </div>
            {ssh.authMethod === "password" && (
              <Field label={t("connect.serverTabs.sshPassword")} htmlFor="ssh-password">
                <PasswordInput id="ssh-password" value={ssh.password} onChange={(v) => setSsh("password", v)} />
              </Field>
            )}
            {ssh.authMethod === "key" && (
              <>
                <Field label={t("connect.serverTabs.privateKey")} htmlFor="ssh-key">
                  <div className="flex gap-2">
                    <Input
                      id="ssh-key"
                      value={ssh.keyPath}
                      onChange={(e) => setSsh("keyPath", e.target.value)}
                      placeholder={t("connect.serverTabs.privateKeyPlaceholder")}
                      spellCheck={false}
                      className="font-mono text-xs"
                      aria-invalid={invalid("ssh-key")}
                    />
                    <Button
                      variant="outline"
                      onClick={async () => {
                        const path = await pickFile("Choose a private key");
                        if (path) setSsh("keyPath", path);
                      }}
                    >
                      <FolderOpen />
                      {t("connect.serverTabs.browse")}
                    </Button>
                  </div>
                </Field>
                <Field label={t("connect.serverTabs.passphrase")} htmlFor="ssh-passphrase">
                  <Input id="ssh-passphrase" type="password" value={ssh.passphrase} onChange={(e) => setSsh("passphrase", e.target.value)} placeholder={t("connect.serverTabs.passphraseHint")} autoComplete="off" />
                </Field>
              </>
            )}
            {ssh.authMethod === "agent" && (
              <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <KeyRound size={13} className="shrink-0" />
                {t("connect.serverTabs.agentNote")}
              </p>
            )}
          </>
        )}
      </div>
    );
  }

  return null;
}
