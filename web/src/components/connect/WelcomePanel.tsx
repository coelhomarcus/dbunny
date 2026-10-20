import { Link, Plus } from "lucide-react";
import { isMac } from "../../lib/platform";
import Button from "../ui/Button";
import { Keys } from "../ui/Kbd";
import { useT } from "../../i18n";

const mod = isMac ? "⌘" : "Ctrl+";

function Shortcut({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <Keys keys={keys} />
      {label}
    </span>
  );
}

/** What the home page shows when no saved connection is picked. */
export default function WelcomePanel({
  hasConnections,
  onNew,
  onOpenUrl,
}: {
  hasConnections: boolean;
  onNew: () => void;
  onOpenUrl: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-10 text-center select-none">
      <img src="/icon.svg" className="size-14 invert dark:invert-0" alt="" />
      <h1 className="mt-4 text-xl font-semibold leading-tight">DBunny</h1>
      <p className="mt-3 max-w-sm text-sm text-muted-foreground">
        {hasConnections ? t("connect.welcomePanel.pickOrCreate") : t("connect.welcomePanel.createFirst")}
      </p>
      <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
        <Button variant="primary" onClick={onNew}>
          <Plus />
          {t("connect.welcomePanel.newConnection")}
        </Button>
        <Button variant="outline" onClick={onOpenUrl}>
          <Link />
          {t("connect.welcomePanel.openUrl")}
        </Button>
      </div>
      <div className="mt-8 flex items-center gap-4 text-xs text-muted-foreground">
        <Shortcut keys={`${mod}T`} label={t("connect.welcomePanel.newTab")} />
        <Shortcut keys={`${mod},`} label={t("connect.welcomePanel.settings")} />
      </div>
    </div>
  );
}
