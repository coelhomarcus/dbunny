import { useEffect, useState } from "react";
import { Clock, Rows3, Square, TriangleAlert } from "lucide-react";
import Spinner from "../ui/Spinner";
import type { QueryResult } from "@/types";
import Button from "../ui/Button";
import ExportMenu from "../ExportMenu";
import { Keys } from "../ui/Kbd";
import { useT } from "../../i18n";

function formatDuration(ms: number) {
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}

/** Elapsed time of the running query, ticking while it runs. */
function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(timer);
  }, []);
  return <span className="tabular-nums">{formatDuration(now - since)}</span>;
}

/** Strip under the results: how the last run went, and exporting its rows. */
export default function ResultsBar({
  result,
  error,
  runningSince,
  onCancel,
  exportName,
}: {
  result: QueryResult | null;
  error: string | null;
  /** When the running query started, or null when nothing runs. */
  runningSince: number | null;
  onCancel: () => void;
  exportName: string;
}) {
  const t = useT();
  return (
    <div className="flex h-8 shrink-0 items-center gap-3 border-t border-border px-3 text-xs text-muted-foreground select-none [&_svg]:size-3.5">
      {runningSince !== null ? (
        <>
          <span className="flex items-center gap-1.5 text-foreground">
            <Spinner />
            {t("query.resultsBar.running")} <Elapsed since={runningSince} />
          </span>
          <Button variant="ghost-muted" size="xs" onClick={onCancel}>
            <Square className="size-2.5! fill-current" />
            {t("common.cancel")}
            <Keys keys="Esc" />
          </Button>
        </>
      ) : error ? (
        <span className="flex items-center gap-1.5 text-destructive-foreground">
          <TriangleAlert />
          {t("query.resultsBar.failed")}
        </span>
      ) : result ? (
        <>
          <span className="flex items-center gap-1.5 text-foreground">
            <Rows3 />
            <span className="tabular-nums">
              {result.rowsAffected != null
                ? t("query.resultsBar.rowsAffected", { count: result.rowsAffected })
                : t("query.resultsBar.rows", { count: result.rowCount })}
              {/* A big change brings back only its first rows */}
              {result.rowsAffected != null && result.rows.length > 0 && result.rows.length < result.rowsAffected && (
                <span className="text-muted-foreground"> · {t("query.resultsBar.firstShown", { count: result.rows.length })}</span>
              )}
            </span>
          </span>
          <span className="flex items-center gap-1.5 tabular-nums">
            <Clock />
            {formatDuration(result.duration)}
          </span>
          {(result.statementCount ?? 1) > 1 && <span>{t("query.resultsBar.statements", { count: result.statementCount ?? 0 })}</span>}
        </>
      ) : (
        <span>{t("query.resultsBar.noResultsYet")}</span>
      )}
      <div className="flex-1" />
      {result && result.columns.length > 0 && runningSince === null && !error && (
        <ExportMenu columns={result.columns} rows={result.rows} defaultName={exportName} side="up" />
      )}
    </div>
  );
}
