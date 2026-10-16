import { createContext } from "react";
import type { SessionApi } from "../lib/api";
import type { ConnectionInfo, SavedTag } from "@/types";
import type { ConnectArgs } from "../lib/connectionDraft";

export interface ConnectionContextValue {
  isConnected: boolean;
  isConnecting: boolean;
  connectionInfo: ConnectionInfo | null;
  savedConnectionName: string | null;
  sessionId: string | null;
  /** API bound to this tab's session. */
  api: SessionApi;
  error: string | null;
  /** Opens this tab's session. `trustHostKey` is an SSH host key fingerprint the user confirmed. */
  connect: (args: ConnectArgs, saved?: SavedTag, trustHostKey?: string) => Promise<void>;
  disconnect: () => Promise<void>;
}

export const ConnectionContext = createContext<ConnectionContextValue | null>(null);
