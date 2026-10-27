import { useState, useCallback, useEffect, useMemo, type ReactNode } from "react";
import { api, createSessionApi } from "../lib/api";
import type { ConnectionInfo, ConnectResponse, InitialSession, SavedTag } from "@/types";
import type { ConnectArgs } from "../lib/connectionDraft";
import { hostKeyProblem } from "../lib/sshHostKey";
import type { TabSessionInfo } from "./workspace";
import { ConnectionContext } from "./connection";
import { addressOf } from "../lib/engines";
import { t } from "../i18n";
import { errorText } from "../lib/appError";

const DISCONNECTED: TabSessionInfo = { title: null, subtitle: null, color: null, icon: null, sessionId: null };

/** One per window tab: owns that tab's database session. */
function infoOf(response: ConnectResponse): ConnectionInfo {
  return {
    database: response.database,
    host: response.host,
    port: response.port,
    serverVersion: response.serverVersion,
    readOnly: response.readOnly,
    engine: response.engine,
    capabilities: response.capabilities,
    endOfSupport: response.endOfSupport ?? null,
  };
}

function tabInfo(response: ConnectResponse, saved?: SavedTag): TabSessionInfo {
  return {
    title: saved?.name || response.database,
    subtitle: addressOf(response),
    color: saved?.color ?? null,
    icon: saved?.icon ?? null,
    sessionId: response.sessionId,
  };
}

export function ConnectionProvider({
  children,
  onSessionChange,
  initialSession,
}: {
  children: ReactNode;
  /** Reports title/status to the window tab bar. */
  onSessionChange: (info: TabSessionInfo) => void;
  /** Starts already connected with this session (opened elsewhere, e.g. another database). */
  initialSession?: InitialSession;
}) {
  const [sessionId, setSessionId] = useState<string | null>(initialSession?.response.sessionId ?? null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionInfo, setConnectionInfo] = useState<ConnectionInfo | null>(
    initialSession ? infoOf(initialSession.response) : null,
  );
  const [savedConnectionName, setSavedConnectionName] = useState<string | null>(initialSession?.saved?.name ?? null);
  const [savedConnectionId, setSavedConnectionId] = useState<string | null>(initialSession?.saved?.id ?? null);

  // The tab bar learns about a starting session once, when the tab opens
  useEffect(() => {
    if (initialSession) onSessionChange(tabInfo(initialSession.response, initialSession.saved));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [error, setError] = useState<string | null>(null);

  const connect = useCallback(
    async (args: ConnectArgs, saved?: SavedTag, trustHostKey?: string) => {
      setIsConnecting(true);
      setError(null);
      try {
        const response =
          args.kind === "sqlite"
            ? await api.connectSqlite(args.path, args.readOnly, args.create)
            : args.kind === "mysql"
              ? await api.connectMysql(args.params, args.ssh, trustHostKey)
              : args.kind === "mariadb"
                ? await api.connectMariadb(args.params, args.ssh, trustHostKey)
                : await api.connect(args.url, args.ssh, trustHostKey);
        setSessionId(response.sessionId);
        setConnectionInfo(infoOf(response));
        setSavedConnectionName(saved?.name || null);
        setSavedConnectionId(saved?.id ?? null);
        onSessionChange(tabInfo(response, saved));
      } catch (err) {
        // An SSH host key to confirm isn't an error to show: the page asks the user instead
        if (hostKeyProblem(err)) throw err;
        const message = errorText(err, t("ui.connectionContext.connectFailed"));
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
      setSavedConnectionId(null);
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
      savedConnectionId,
      sessionId,
      api: sessionApi,
      error,
      connect,
      disconnect,
    }),
    [sessionId, isConnecting, connectionInfo, savedConnectionName, savedConnectionId, sessionApi, error, connect, disconnect],
  );

  return <ConnectionContext.Provider value={value}>{children}</ConnectionContext.Provider>;
}
