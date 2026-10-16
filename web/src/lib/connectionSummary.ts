import type { SavedConnection } from "@/types";
// With the extension, so Node's test runner can load it too
import { isSocketHost } from "./connectionUrl.ts";

/** Hides a value character by character (host names, database names on screen). */
export const mask = (value: string) => value.replace(/./g, "•");

/** What the home page says about a saved connection. Never its password. */
export interface ConnectionSummary {
  engineLabel: string;
  address: string;
  user: string;
  ssl: boolean;
  /** `user@host` (and `:port` when not 22) of the SSH server it goes through. */
  sshVia: string | null;
  readOnly: boolean;
}

/** The last segment of a path, whichever separator it uses. */
export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function summaryOf(conn: SavedConnection): ConnectionSummary {
  if (conn.engine === "sqlite") {
    return { engineLabel: "SQLite", address: conn.path, user: "", ssl: false, sshVia: null, readOnly: conn.readOnly ?? false };
  }
  const socket = isSocketHost(conn.host);
  const ssh = conn.ssh;
  return {
    engineLabel: "PostgreSQL",
    address: socket ? `${conn.host} (socket) / ${conn.database}` : `${conn.host}:${conn.port}/${conn.database}`,
    user: conn.user,
    ssl: conn.ssl && !socket,
    sshVia: ssh && !socket ? `${ssh.user}@${ssh.host}${ssh.port && ssh.port !== 22 ? `:${ssh.port}` : ""}` : null,
    readOnly: conn.readOnly ?? false,
  };
}
