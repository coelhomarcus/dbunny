import type { Engine, SshConfig } from "./connection";

/** SSL settings beyond on/off: how the server is checked and the certificate files. */
export interface TlsSettings {
  mode: SslMode;
  rootCert: string;
  cert: string;
  key: string;
}

/** "require" encrypts; the verify modes also check the server's certificate (and name). */
export type SslMode = "require" | "verify-ca" | "verify-full";

export interface SavedConnection {
  id: string;
  name: string;
  /** Which database it is; decides what the other details mean. */
  engine: Engine;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  /** Connect through this SSH server; null for a direct connection. */
  ssh: SshConfig | null;
  tls?: TlsSettings | null;
  ssl: boolean;
  /** Every transaction starts read-only, so the server refuses writes. */
  readOnly: boolean;
  /** The database file of a SQLite connection; empty for the others. */
  path: string;
  color: string;
  /** An id from CONNECTION_ICONS; empty for the default. */
  icon: string;
  createdAt: string;
  updatedAt: string;
}

export interface SavedConnectionInput {
  id?: string;
  name: string;
  engine: Engine;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  ssh: SshConfig | null;
  tls: TlsSettings | null;
  ssl: boolean;
  readOnly: boolean;
  /** The database file of a SQLite connection; empty for the others. */
  path: string;
  color: string;
  icon: string;
}
