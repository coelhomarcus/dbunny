import type { Capabilities } from "./capabilities";

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
  engine: "postgres";
  /** What the session supports. */
  capabilities: Capabilities;
}

export interface ConnectionInfo {
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  readOnly: boolean;
  engine: "postgres";
  capabilities: Capabilities;
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
  name: string;
  color?: string | null;
  icon?: string | null;
}

/** A session opened elsewhere (e.g. another database) that a new tab starts with. */
export interface InitialSession {
  response: ConnectResponse;
  saved?: SavedTag;
}
