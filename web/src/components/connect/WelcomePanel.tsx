import { Link, Plus } from "lucide-react";
import { isMac } from "../../lib/platform";
import Button from "../ui/Button";

const mod = isMac ? "⌘" : "Ctrl+";

function Shortcut({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <kbd className="rounded-md border border-border bg-muted px-1.5 py-0.5 font-sans text-[11px] text-foreground">{keys}</kbd>
      {label}
    </span>
  );
}

/** What the home page shows when no saved connection is picked. */
export default function WelcomePanel({
  version,
  hasConnections,
  onNew,
  onOpenUrl,
}: {
  version: string | null;
  hasConnections: boolean;
  onNew: () => void;
  onOpenUrl: () => void;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-10 text-center select-none">
      <img src="/icon.svg" className="size-14 invert dark:invert-0" alt="" />
      <h1 className="mt-4 text-xl font-semibold leading-tight">DBunny</h1>
      {version && <p className="mt-0.5 text-xs text-muted-foreground">Version {version}</p>}
      <p className="mt-3 max-w-sm text-sm text-muted-foreground">
        {hasConnections ? "Pick a connection on the left, or create a new one." : "Create your first connection to get started."}
      </p>
      <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
        <Button variant="primary" onClick={onNew}>
          <Plus />
          New connection
        </Button>
        <Button variant="outline" onClick={onOpenUrl}>
          <Link />
          Open connection URL…
        </Button>
      </div>
      <div className="mt-8 flex items-center gap-4 text-xs text-muted-foreground">
        <Shortcut keys={`${mod}T`} label="New tab" />
        <Shortcut keys={`${mod},`} label="Settings" />
      </div>
    </div>
  );
}
