import { LogOut } from "lucide-react";
import type { ConnectionInfo } from "@/types";
import ThemeToggle from "../ui/ThemeToggle";

const mask = (value: string) => value.replace(/./g, "•");

/** Bottom bar of a connected tab: who we're talking to, plus global actions. */
export default function StatusBar({
  connectionInfo,
  revealed,
  onDisconnect,
}: {
  connectionInfo: ConnectionInfo | null;
  /** Show host/database in clear text (they're masked by default). */
  revealed: boolean;
  onDisconnect: () => void;
}) {
  const host = connectionInfo ? `${connectionInfo.host}:${connectionInfo.port}` : "";
  const database = connectionInfo?.database ?? "";

  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-frame-line bg-chrome px-3 text-[11px] text-muted-foreground select-none">
      <span className="flex items-center gap-1.5 text-foreground">
        <span className="size-1.5 rounded-full bg-success" />
        Connected
      </span>
      <span className="font-mono">{revealed ? host : mask(host)}</span>
      <span className="font-mono">{revealed ? database : mask(database)}</span>
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
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-frame-line bg-chrome px-3 text-[11px] text-muted-foreground select-none">
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
