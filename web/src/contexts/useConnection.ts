import { useContext } from "react";
import { ConnectionContext } from "./connection";

export function useConnection() {
  const ctx = useContext(ConnectionContext);
  if (!ctx) throw new Error("useConnection must be used within ConnectionProvider");
  return ctx;
}

/** API bound to the current window tab's database session. */
export function useApi() {
  return useConnection().api;
}
