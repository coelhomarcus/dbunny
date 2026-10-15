import { createContext } from "react";
import type { SessionApi } from "../lib/api";
import type { ConnectionInfo } from "@/types";

export interface ConnectionContextValue {
  isConnected: boolean;
  isConnecting: boolean;
  connectionInfo: ConnectionInfo | null;
  savedConnectionName: string | null;
  sessionId: string | null;
  /** API bound to this tab's session. */
  api: SessionApi;
  error: string | null;
  connect: (url: string, saved?: { name: string; color?: string | null }) => Promise<void>;
  disconnect: () => Promise<void>;
}

export const ConnectionContext = createContext<ConnectionContextValue | null>(null);
