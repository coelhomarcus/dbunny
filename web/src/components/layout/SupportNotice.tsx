import { useState } from "react";
import { TriangleAlert } from "lucide-react";
import Button from "../ui/Button";
import { useConnection } from "../../contexts/useConnection";
import { useFormat, useT } from "../../i18n";
import { engineLabel } from "../../lib/engines";
import { readSeen, rememberSeen, seenValue, shouldShowSupportNotice, supportNoticeKey } from "../../lib/supportNotice";
import type { ConnectionInfo } from "@/types";

/** The browser's storage, or null where reading it throws. */
function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** "PostgreSQL 13 reached its end of support on 13 Nov 2025…", for the notice and the status bar. */
export function useEndOfSupportText(info: ConnectionInfo | null): string | null {
  const t = useT();
  const format = useFormat();
  if (!info?.endOfSupport) return null;
  return t("ui.supportNotice.pastEndOfSupport", {
    engine: engineLabel(info.engine),
    version: info.endOfSupport.line,
    date: format.date(new Date(`${info.endOfSupport.date}T00:00:00`), { dateStyle: "medium" }),
  });
}

/** Shown once per connection and version line when the server's version is past its end of support. */
export default function SupportNotice() {
  const t = useT();
  const { connectionInfo, savedConnectionId } = useConnection();
  const text = useEndOfSupportText(connectionInfo);
  const key = connectionInfo ? supportNoticeKey(savedConnectionId, connectionInfo) : null;
  const [dismissed, setDismissed] = useState<string | null>(null);
  if (!connectionInfo?.endOfSupport || !key || !text) return null;
  const value = seenValue(connectionInfo.engine, connectionInfo.endOfSupport);
  if (dismissed === value || !shouldShowSupportNotice(connectionInfo.endOfSupport, readSeen(storage(), key), connectionInfo.engine)) return null;

  return (
    <div role="status" className="flex shrink-0 items-center gap-2 border-b border-border bg-warning/10 px-3 py-1.5 text-xs text-warning-foreground">
      <TriangleAlert size={13} className="shrink-0" />
      <span className="min-w-0 flex-1">{text}</span>
      <Button
        size="sm"
        variant="ghost"
        onClick={() => {
          rememberSeen(storage(), key, value);
          setDismissed(value);
        }}
      >
        {t("ui.supportNotice.gotIt")}
      </Button>
    </div>
  );
}
