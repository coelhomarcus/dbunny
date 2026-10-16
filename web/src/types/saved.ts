import type { SshConfig } from "./connection";

export interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  /** Connect through this SSH server; null for a direct connection. */
  ssh: SshConfig | null;
  ssl: boolean;
  /** Every transaction starts read-only, so the server refuses writes. */
  readOnly: boolean;
  color: string;
  createdAt: string;
  updatedAt: string;
}

export interface SavedConnectionInput {
  id?: string;
  name: string;
  host: string;
  port: number;
  database: string;
  user: string;
  password: string;
  ssh: SshConfig | null;
  ssl: boolean;
  readOnly: boolean;
  color: string;
}
