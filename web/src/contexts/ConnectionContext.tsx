import { useState, useCallback, useMemo, type ReactNode } from "react";
import { api, createSessionApi } from "../lib/api";
import type { ConnectionInfo, SshConfig } from "@/types";
import { hostKeyProblem } from "../lib/sshHostKey";
import type { TabSessionInfo } from "./workspace";
import { ConnectionContext } from "./connection";

const DISCONNECTED: TabSessionInfo = { title: null, subtitle: null, color: null, sessionId: null };

/** One per window tab: owns that tab's database session. */
export function ConnectionProvider({
  children,
  onSessionChange,
}: {
  children: ReactNode;
  /** Reports title/status to the window tab bar. */
  onSessionChange: (info: TabSessionInfo) => void;
}) {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionInfo, setConnectionInfo] = useState<ConnectionInfo | null>(null);
  const [savedConnectionName, setSavedConnectionName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const connect = useCallback(
    async (
      url: string,
      saved?: { name: string; color?: string | null },
      tunnel?: { ssh?: SshConfig | null; trustHostKey?: string },
    ) => {
      setIsConnecting(true);
      setError(null);
      try {
        const response = await api.connect(url, tunnel?.ssh, tunnel?.trustHostKey);
        const info = {
          database: response.database,
          host: response.host,
          port: response.port,
          serverVersion: response.serverVersion,
          readOnly: response.readOnly,
        };
        setSessionId(response.sessionId);
        setConnectionInfo(info);
        setSavedConnectionName(saved?.name || null);
        onSessionChange({
          title: saved?.name || info.database,
          subtitle: `${info.host}:${info.port}/${info.database}`,
          color: saved?.color ?? null,
          sessionId: response.sessionId,
        });
      } catch (err) {
        // An SSH host key to confirm isn't an error to show: the page asks the user instead
        if (hostKeyProblem(err)) throw err;
        const message = typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to connect";
        setError(message);
        throw err;
      } finally {
        setIsConnecting(false);
      }
    },
    [onSessionChange],
  );

  const disconnect = useCallback(async () => {
    try {
      if (sessionId) await api.disconnect(sessionId);
    } catch {
      // the session is dropped locally either way
    } finally {
      setSessionId(null);
      setConnectionInfo(null);
      setSavedConnectionName(null);
      onSessionChange(DISCONNECTED);
    }
  }, [sessionId, onSessionChange]);

  const sessionApi = useMemo(() => createSessionApi(sessionId), [sessionId]);

  const value = useMemo(
    () => ({
      isConnected: sessionId !== null,
      isConnecting,
      connectionInfo,
      savedConnectionName,
      sessionId,
      api: sessionApi,
      error,
      connect,
      disconnect,
    }),
    [sessionId, isConnecting, connectionInfo, savedConnectionName, sessionApi, error, connect, disconnect],
  );

  return <ConnectionContext.Provider value={value}>{children}</ConnectionContext.Provider>;
}
