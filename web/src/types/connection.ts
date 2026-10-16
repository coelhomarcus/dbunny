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
}

export interface ConnectionInfo {
  database: string;
  host: string;
  port: number;
  serverVersion: string;
  readOnly: boolean;
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
