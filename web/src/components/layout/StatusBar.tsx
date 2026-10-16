import { Eye, EyeOff, Lock, LogOut } from "lucide-react";
import type { ConnectionInfo } from "@/types";
import ThemeToggle from "../ui/ThemeToggle";

const mask = (value: string) => value.replace(/./g, "•");

/** Bottom bar of a connected tab: who we're talking to, plus global actions. */
export default function StatusBar({
  connectionInfo,
  revealed,
  onToggleReveal,
  onDisconnect,
}: {
  connectionInfo: ConnectionInfo | null;
  /** Show host/database in clear text (they're masked by default). */
  revealed: boolean;
  onToggleReveal: () => void;
  onDisconnect: () => void;
}) {
  const host = connectionInfo ? `${connectionInfo.host}:${connectionInfo.port}` : "";
  const database = connectionInfo?.database ?? "";

  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 px-1.5 text-[11px] text-muted-foreground select-none">
      <span className="flex items-center gap-1.5 text-foreground">
        <span className="size-1.5 rounded-full bg-success" />
        Connected
      </span>
      {connectionInfo?.readOnly && (
        <span className="flex items-center gap-1 rounded bg-warning/12 px-1.5 py-px font-medium text-warning-foreground">
          <Lock size={10} />
          Read-only
        </span>
      )}
      {/* The toggle sits on the details it reveals */}
      <button
        onClick={onToggleReveal}
        aria-label={revealed ? "Hide connection details" : "Show connection details"}
        className="group flex h-5 items-center gap-2 rounded-md px-1.5 font-mono transition-colors cursor-pointer hover:bg-accent hover:text-foreground"
      >
        <span>{revealed ? host : mask(host)}</span>
        <span>{revealed ? database : mask(database)}</span>
        {revealed ? <EyeOff size={11} className="shrink-0" /> : <Eye size={11} className="shrink-0" />}
      </button>
      <div className="ml-auto flex items-center gap-1">
        {connectionInfo?.serverVersion && <span className="mr-1.5">PostgreSQL {connectionInfo.serverVersion.split(" ")[0]}</span>}
        <ThemeToggle side="top" size="icon-xs" />
        <button
          onClick={onDisconnect}
          className="flex h-5 items-center gap-1 rounded-md px-1.5 hover:bg-destructive/10 hover:text-destructive-foreground transition-colors cursor-pointer"
        >
          <LogOut size={11} />
          Disconnect
        </button>
      </div>
    </footer>
  );
}

/** Bottom bar of a tab that isn't connected yet (the home screen). */
export function IdleStatusBar({ version }: { version: string | null }) {
  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 px-1.5 text-[11px] text-muted-foreground select-none">
      <span className="flex items-center gap-1.5">
        <span className="size-1.5 rounded-full bg-muted-foreground/50" />
        Not connected
      </span>
      <div className="ml-auto flex items-center gap-1">
        {version && <span className="mr-1.5">DBunny {version}</span>}
        <ThemeToggle side="top" size="icon-xs" />
      </div>
    </footer>
  );
}
