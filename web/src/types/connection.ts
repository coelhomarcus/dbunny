import type { Capabilities } from "./capabilities";
import type { TlsSettings } from "./saved";

/** The database a connection talks to. */
export type Engine = "postgres" | "sqlite" | "mysql" | "mariadb";

/** The server's version line is past its vendor's end of support: `line` as "13" or "8.0", `date` the last day (YYYY-MM-DD). */
export interface EndOfSupport {
  line: string;
  date: string;
}

export interface ConnectRequest {
  connectionUrl: string;
}

export interface ConnectResponse {
  sessionId: string;
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  /** Asked for read-only, or the server is a read replica. */
  readOnly: boolean;
  engine: Engine;
  /** What the session supports. */
  capabilities: Capabilities;
  /** Set only when the version is past its end of support. */
  endOfSupport: EndOfSupport | null;
}

export interface ConnectionInfo {
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  readOnly: boolean;
  engine: Engine;
  capabilities: Capabilities;
  endOfSupport: EndOfSupport | null;
}

/** The connection form of a MySQL or MariaDB server, as `connect_mysql` and `connect_mariadb` take it. */
export interface MysqlConnectParams {
  host: string;
  port: number;
  /** Empty opens the first database the user can see. */
  database: string;
  user: string;
  password: string;
  readOnly: boolean;
  /** Null: no TLS. */
  tls: TlsSettings | null;
}

export type SshAuthMethod = "password" | "key" | "agent";

/** SSH server the database is reached through. */
export interface SshConfig {
  host: string;
  port: number;
  user: string;
  authMethod: SshAuthMethod;
  password: string;
  /** Private key file, for "key". */
  keyPath: string;
  /** Passphrase of an encrypted private key. */
  passphrase: string;
}

/** Why the SSH server's key wasn't accepted; the user decides whether to trust it. */
export interface SshHostKeyProblem {
  /** "unknown": first time; "changed": differs from the key seen before. */
  status: "unknown" | "changed";
  host: string;
  port: number;
  fingerprint: string;
  algorithm: string;
  /** The mismatch is with ~/.ssh/known_hosts, which DBunny doesn't change. */
  inUserKnownHosts: boolean;
}

/** A database of the connected server, for switching. */
export interface DatabaseInfo {
  name: string;
  sizeBytes: number | null;
  isCurrent: boolean;
  /** The user may connect to it. */
  canConnect: boolean;
}

/** How a saved connection presents itself: its name, colour and icon. */
export interface SavedTag {
  /** The id of the saved connection; missing for one that was never saved. */
  id?: string;
  name: string;
  color?: string | null;
  icon?: string | null;
}

/** A session opened elsewhere (e.g. another database) that a new tab starts with. */
export interface InitialSession {
  response: ConnectResponse;
  saved?: SavedTag;
}
