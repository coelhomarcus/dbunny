import type { Engine, SavedConnection, SavedConnectionInput, SavedTag, SshConfig } from "@/types";
// With the extension, so Node's test runner can load it too
import { type ConnectionFields, buildConnectionUrl, isSocketHost, parseConnectionUrl, tlsFieldsOf, tlsSettingsOf } from "./connectionUrl.ts";

/**
 * What the connection modal edits: who the connection is (name, colour, icon) and how to
 * reach it. Kept apart from the components so validation and conversions are testable.
 */

export const DEFAULT_COLOR = "#3b82f6";
// Colour tags to tell environments apart (prod, staging, local...)
export const COLOR_PRESETS = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899"];

export type { Engine };

export type ConnectionTab = "connection" | "ssl" | "ssh" | "options";

/** The SSH form keeps the port as typed; it becomes a number when used. */
export type SshFields = Omit<SshConfig, "port"> & { port: string };
export const EMPTY_SSH: SshFields = { host: "", port: "", user: "", authMethod: "password", password: "", keyPath: "", passphrase: "" };

export interface Identity {
  name: string;
  color: string;
  /** An id from CONNECTION_ICONS; empty for the default. */
  icon: string;
}

export interface ConnectionDraft {
  /** The saved connection being edited; null for a new one. */
  id: string | null;
  engine: Engine;
  identity: Identity;
  fields: ConnectionFields;
  sshEnabled: boolean;
  ssh: SshFields;
}

/** A field left empty that the action needs, and the tab it's on. */
export interface MissingField {
  tab: ConnectionTab | "identity";
  fieldId: string;
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

export function newDraft(engine: Engine): ConnectionDraft {
  return {
    id: null,
    engine,
    identity: { name: "", color: DEFAULT_COLOR, icon: "" },
    fields: EMPTY_FIELDS,
    sshEnabled: false,
    ssh: EMPTY_SSH,
  };
}

export function draftFromSaved(conn: SavedConnection): ConnectionDraft {
  return {
    id: conn.id,
    // Saved objects from before the engine was stored are PostgreSQL ones
    engine: conn.engine ?? "postgres",
    identity: { name: conn.name, color: conn.color || DEFAULT_COLOR, icon: conn.icon ?? "" },
    fields: {
      host: conn.host,
      port: conn.port.toString(),
      database: conn.database,
      user: conn.user,
      password: conn.password,
      ssl: conn.ssl,
      readOnly: conn.readOnly ?? false,
      ...tlsFieldsOf(conn.tls),
    },
    sshEnabled: Boolean(conn.ssh),
    // The default port shows as empty, like when it was typed
    ssh: conn.ssh ? { ...conn.ssh, port: conn.ssh.port && conn.ssh.port !== 22 ? String(conn.ssh.port) : "" } : EMPTY_SSH,
  };
}

/** Fills the server details from a postgres:// URL, keeping who the connection is. Null if it isn't one. */
export function applyUrl(draft: ConnectionDraft, url: string): ConnectionDraft | null {
  const text = url.trim();
  if (!/^postgres(ql)?:\/\//i.test(text)) return null;
  const parsed = parseConnectionUrl(text);
  if (!parsed.host) return null;
  return { ...draft, fields: { ...draft.fields, ...parsed } };
}

export function tabsFor(engine: Engine): ConnectionTab[] {
  switch (engine) {
    case "postgres":
      return ["connection", "ssl", "ssh", "options"];
  }
}

/** The optional tabs that are turned on. SSL and SSH don't apply to a socket. */
export function configuredTabs(draft: ConnectionDraft): Set<ConnectionTab> {
  const tabs = new Set<ConnectionTab>();
  const socket = isSocketHost(draft.fields.host);
  if (draft.fields.ssl && !socket) tabs.add("ssl");
  if (draft.sshEnabled && !socket) tabs.add("ssh");
  if (draft.fields.readOnly) tabs.add("options");
  return tabs;
}

/** Empty fields that connecting (or saving, which also needs a name) requires, in screen order. */
export function missingFields(draft: ConnectionDraft, purpose: "connect" | "save"): MissingField[] {
  const missing: MissingField[] = [];
  const { fields, ssh } = draft;
  if (purpose === "save" && !draft.identity.name.trim()) missing.push({ tab: "identity", fieldId: "conn-name" });
  if (!fields.host.trim()) missing.push({ tab: "connection", fieldId: "conn-host" });
  if (!fields.database.trim()) missing.push({ tab: "connection", fieldId: "conn-db" });
  if (draft.sshEnabled && !isSocketHost(fields.host)) {
    if (!ssh.host.trim()) missing.push({ tab: "ssh", fieldId: "ssh-host" });
    if (!ssh.user.trim()) missing.push({ tab: "ssh", fieldId: "ssh-user" });
    if (ssh.authMethod === "key" && !ssh.keyPath.trim()) missing.push({ tab: "ssh", fieldId: "ssh-key" });
  }
  return missing;
}

/** The SSH settings to connect with, or null for a direct connection. */
export function sshConfigOf(draft: ConnectionDraft): SshConfig | null {
  if (!draft.sshEnabled || isSocketHost(draft.fields.host)) return null;
  const { ssh } = draft;
  return { ...ssh, host: ssh.host.trim(), user: ssh.user.trim(), port: parseInt(ssh.port || "22", 10) };
}

export function connectionUrlOf(draft: ConnectionDraft): string {
  return buildConnectionUrl(draft.fields);
}

export function saveInputOf(draft: ConnectionDraft): SavedConnectionInput {
  const { fields, identity } = draft;
  return {
    id: draft.id ?? undefined,
    name: identity.name.trim(),
    engine: draft.engine,
    host: fields.host.trim(),
    port: parseInt(fields.port || "5432", 10),
    database: fields.database.trim(),
    user: fields.user.trim(),
    password: fields.password,
    ssl: fields.ssl,
    readOnly: fields.readOnly,
    tls: tlsSettingsOf(fields),
    ssh: sshConfigOf(draft),
    color: identity.color,
    icon: identity.icon,
  };
}

/** How the tab presents the connection; nothing without a name (it isn't a saved one then). */
export function savedTagOf(draft: ConnectionDraft): SavedTag | undefined {
  const name = draft.identity.name.trim();
  return name ? { name, color: draft.identity.color, icon: draft.identity.icon } : undefined;
}
