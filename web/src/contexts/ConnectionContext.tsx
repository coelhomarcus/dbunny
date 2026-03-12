import { createContext, useState, useCallback, type ReactNode } from "react";
import { api, setSessionId } from "../lib/api";
import type { ConnectionInfo } from "@/types";

interface ConnectionContextValue {
  isConnected: boolean;
  isConnecting: boolean;
  connectionInfo: ConnectionInfo | null;
  savedConnectionName: string | null;
  error: string | null;
  connect: (url: string, savedName?: string) => Promise<void>;
  disconnect: () => Promise<void>;
}

export const ConnectionContext = createContext<ConnectionContextValue | null>(null);

export function ConnectionProvider({ children }: { children: ReactNode }) {
  const [isConnected, setIsConnected] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionInfo, setConnectionInfo] = useState<ConnectionInfo | null>(null);
  const [savedConnectionName, setSavedConnectionName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const connect = useCallback(async (url: string, savedName?: string) => {
    setIsConnecting(true);
    setError(null);
    try {
      const response = await api.connect(url);
      setSessionId(response.sessionId);
      setConnectionInfo({
        database: response.database,
        host: response.host,
        port: response.port,
      });
      setSavedConnectionName(savedName || null);
      setIsConnected(true);
    } catch (err) {
      const message = typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to connect";
      setError(message);
      throw err;
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await api.disconnect();
    } catch {
      // ignore disconnect errors
    } finally {
      setSessionId(null);
      setIsConnected(false);
      setConnectionInfo(null);
      setSavedConnectionName(null);
    }
  }, []);

  return (
    <ConnectionContext.Provider
      value={{ isConnected, isConnecting, connectionInfo, savedConnectionName, error, connect, disconnect }}
    >
      {children}
    </ConnectionContext.Provider>
  );
}
