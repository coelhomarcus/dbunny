import { useContext } from "react";
import { ConnectionContext } from "./ConnectionContext";

export function useConnection() {
  const ctx = useContext(ConnectionContext);
  if (!ctx) throw new Error("useConnection must be used within ConnectionProvider");
  return ctx;
}
