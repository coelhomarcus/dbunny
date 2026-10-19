import type { Engine, MysqlConnectParams, SavedConnection, SavedConnectionInput, SavedTag, SshConfig } from "@/types";
// With the extension, so Node's test runner can load it too
import { type ConnectionFields, buildConnectionUrl, isSocketHost, parseConnectionUrl, parseMysqlUrl, tlsFieldsOf, tlsSettingsOf } from "./connectionUrl.ts";

/**
 * What the connection modal edits: who the connection is (name, colour, icon) and how to
 * reach it. Kept apart from the components so validation and conversions are testable.
 */

export const DEFAULT_COLOR = "#3b82f6";
// Colour tags to tell environments apart (prod, staging, local...)
export const COLOR_PRESETS = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899"];

export type { Engine };

export type ConnectionTab = "connection" | "ssl" | "ssh" | "options" | "file";

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
  /** SQLite: the database file. */
  sqlitePath: string;
  /** SQLite: the file doesn't exist yet and is made when connecting (a new database). */
  sqliteCreate: boolean;
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
    // MySQL shows its port filled in; PostgreSQL leaves it to the placeholder
    fields: engine === "mysql" ? { ...EMPTY_FIELDS, port: "3306" } : EMPTY_FIELDS,
    sshEnabled: false,
    ssh: EMPTY_SSH,
    sqlitePath: "",
    sqliteCreate: false,
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
    sqlitePath: conn.path ?? "",
    sqliteCreate: false,
  };
}

/**
 * Fills the server details from a postgres:// URL (or a mysql:// one for a MySQL draft), keeping
 * who the connection is. Null if the URL isn't one of this draft's database.
 */
export function applyUrl(draft: ConnectionDraft, url: string): ConnectionDraft | null {
  // A URL only fills the form of its own database; it never changes what kind of connection this is
  const text = url.trim();
  let parsed: Partial<ConnectionFields>;
  if (draft.engine === "postgres" && /^postgres(ql)?:\/\//i.test(text)) parsed = parseConnectionUrl(text);
  else if (draft.engine === "mysql" && /^mysql:\/\//i.test(text)) parsed = parseMysqlUrl(text);
  else return null;
  if (!parsed.host) return null;
  return { ...draft, fields: { ...draft.fields, ...parsed } };
}

/** The draft a connection URL describes, for whichever server database its scheme names. Null if it isn't one. */
export function draftFromUrl(url: string): ConnectionDraft | null {
  for (const engine of ["postgres", "mysql"] as const) {
    const draft = applyUrl(newDraft(engine), url);
    if (draft) return draft;
  }
  return null;
}

/** What the import-from-URL dialog says: for one database, or (with none) for any server database. */
export function urlHints(engine: Engine | undefined): { description: string; placeholder: string; invalid: string } {
  switch (engine) {
    case "mysql":
      return {
        description: "Paste a MySQL connection string to fill in the form.",
        placeholder: "mysql://user:password@host:3306/database",
        invalid: "That doesn't look like a mysql:// connection URL.",
      };
    case "postgres":
      return {
        description: "Paste a PostgreSQL connection string to fill in the form.",
        placeholder: "postgresql://user:password@host:5432/database",
        invalid: "That doesn't look like a postgres:// connection URL.",
      };
    default:
      return {
        description: "Paste a PostgreSQL or MySQL connection string to fill in the form.",
        placeholder: "postgresql://user:password@host:5432/database",
        invalid: "That doesn't look like a postgres:// or mysql:// connection URL.",
      };
  }
}

export function tabsFor(engine: Engine): ConnectionTab[] {
  switch (engine) {
    case "postgres":
    case "mysql":
      return ["connection", "ssl", "ssh", "options"];
    case "sqlite":
      return ["file", "options"];
  }
}

/** The optional tabs that are turned on. SSL and SSH don't apply to a socket. */
export function configuredTabs(draft: ConnectionDraft): Set<ConnectionTab> {
  const tabs = new Set<ConnectionTab>();
  if (draft.engine === "sqlite") {
    if (draft.fields.readOnly) tabs.add("options");
    return tabs;
  }
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
  if (draft.engine === "sqlite") {
    if (!draft.sqlitePath.trim()) missing.push({ tab: "file", fieldId: "sqlite-path" });
    return missing;
  }
  if (!fields.host.trim()) missing.push({ tab: "connection", fieldId: "conn-host" });
  // A MySQL session opens the first database it finds when none is named
  if (draft.engine !== "mysql" && !fields.database.trim()) missing.push({ tab: "connection", fieldId: "conn-db" });
  if (draft.sshEnabled && !isSocketHost(fields.host)) {
    if (!ssh.host.trim()) missing.push({ tab: "ssh", fieldId: "ssh-host" });
    if (!ssh.user.trim()) missing.push({ tab: "ssh", fieldId: "ssh-user" });
    if (ssh.authMethod === "key" && !ssh.keyPath.trim()) missing.push({ tab: "ssh", fieldId: "ssh-key" });
  }
  return missing;
}

/** The SSH settings to connect with, or null for a direct connection. */
export function sshConfigOf(draft: ConnectionDraft): SshConfig | null {
  if (draft.engine === "sqlite" || !draft.sshEnabled || isSocketHost(draft.fields.host)) return null;
  const { ssh } = draft;
  return { ...ssh, host: ssh.host.trim(), user: ssh.user.trim(), port: parseInt(ssh.port || "22", 10) };
}

export function connectionUrlOf(draft: ConnectionDraft): string {
  return buildConnectionUrl(draft.fields);
}

/** What to open a session with: a URL (and SSH) for PostgreSQL, the fields for MySQL, a file for SQLite. */
export type ConnectArgs =
  | { kind: "postgres"; url: string; ssh: SshConfig | null }
  | { kind: "mysql"; params: MysqlConnectParams; ssh: SshConfig | null }
  | { kind: "sqlite"; path: string; readOnly: boolean; create: boolean };

/** The form of a MySQL draft as `connect_mysql` takes it. SSL means nothing over a local socket. */
export function mysqlParamsOf(draft: ConnectionDraft): MysqlConnectParams {
  const { fields } = draft;
  return {
    host: fields.host.trim(),
    port: parseInt(fields.port || "3306", 10),
    database: fields.database.trim(),
    user: fields.user.trim(),
    password: fields.password,
    readOnly: fields.readOnly,
    tls: isSocketHost(fields.host) ? null : tlsSettingsOf(fields),
  };
}

export function connectArgsOf(draft: ConnectionDraft): ConnectArgs {
  if (draft.engine === "sqlite") {
    return { kind: "sqlite", path: draft.sqlitePath.trim(), readOnly: draft.fields.readOnly, create: draft.sqliteCreate };
  }
  if (draft.engine === "mysql") return { kind: "mysql", params: mysqlParamsOf(draft), ssh: sshConfigOf(draft) };
  return { kind: "postgres", url: connectionUrlOf(draft), ssh: sshConfigOf(draft) };
}

export function saveInputOf(draft: ConnectionDraft): SavedConnectionInput {
  const { fields, identity } = draft;
  if (draft.engine === "sqlite") {
    // The backend asks for the server fields whatever the database is; a file has none
    return {
      id: draft.id ?? undefined,
      name: identity.name.trim(),
      engine: "sqlite",
      path: draft.sqlitePath.trim(),
      host: "",
      port: 0,
      database: "",
      user: "",
      password: "",
      ssl: false,
      readOnly: fields.readOnly,
      tls: null,
      ssh: null,
      color: identity.color,
      icon: identity.icon,
    };
  }
  return {
    path: "",
    id: draft.id ?? undefined,
    name: identity.name.trim(),
    engine: draft.engine,
    host: fields.host.trim(),
    port: parseInt(fields.port || (draft.engine === "mysql" ? "3306" : "5432"), 10),
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
