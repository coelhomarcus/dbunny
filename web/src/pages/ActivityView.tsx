import { useCallback, useEffect, useState } from "react";
import { Activity, Pause, Play, RefreshCw, Square, TriangleAlert, XCircle } from "lucide-react";
import type { ServerActivity, ServerSession } from "@/types";
import { useConnection } from "../contexts/useConnection";
import { useTab } from "../contexts/workspace";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { TableSkeleton } from "../components/Skeleton";
import Badge from "../components/ui/Badge";
import Button from "../components/ui/Button";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import { cn } from "../lib/cn";

const REFRESH_MS = 3000;

function duration(seconds: number | null) {
  if (seconds === null) return "—";
  if (seconds < 1) return `${Math.round(seconds * 1000)} ms`;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ${Math.floor(seconds % 60)} s`;
  return `${Math.floor(seconds / 3600)} h ${Math.floor((seconds % 3600) / 60)} min`;
}

const STATE_BADGE: Record<string, "success" | "warning" | "destructive" | "neutral"> = {
  active: "success",
  "idle in transaction": "warning",
  "idle in transaction (aborted)": "destructive",
};

/** The server's sessions, refreshed every few seconds, with cancel and terminate. */
export default function ActivityView() {
  const { api, connectionInfo } = useConnection();
  const { isActive } = useTab();
  const readOnly = connectionInfo?.readOnly ?? false;
  const [scope, setScope] = useState<"database" | "all">("database");
  const [activity, setActivity] = useState<ServerActivity | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [confirm, setConfirm] = useState<{ session: ServerSession; terminate: boolean } | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setActivity(await api.getServerActivity(scope === "all"));
      setError(null);
    } catch (err) {
      setError(typeof err === "string" ? err : "Couldn't read the server activity");
    }
  }, [api, scope]);

  // Loads when opened and when the scope changes...
  useEffect(() => {
    const first = setTimeout(load, 0);
    return () => clearTimeout(first);
  }, [load]);

  // ...then polls, only while visible and not paused
  useEffect(() => {
    if (paused || !isActive) return;
    const timer = setInterval(load, REFRESH_MS);
    return () => clearInterval(timer);
  }, [load, paused, isActive]);

  async function signal(session: ServerSession, terminate: boolean) {
    setConfirm(null);
    try {
      const sent = await api.signalBackend(session.pid, terminate);
      setMessage(sent ? `${terminate ? "Ended session" : "Cancelled the query of"} ${session.pid}` : `Session ${session.pid} had already finished`);
    } catch (err) {
      setMessage(typeof err === "string" ? err : "It didn't work");
    }
    load();
  }

  const sessions = activity?.sessions ?? [];
  const blocking = new Set(sessions.flatMap((s) => s.blockedBy));

  return (
    <div className="h-full flex flex-col">
      <PageHeader className="justify-between">
        <HeaderTitle label="Server" icon={<Activity />}>
          Activity
        </HeaderTitle>
        <div className="flex items-center gap-2">
          <SegmentedTabs
            value={scope}
            onChange={setScope}
            options={[
              { value: "database", label: "This database" },
              { value: "all", label: "All databases" },
            ]}
          />
          <Button variant="ghost-muted" size="sm" onClick={() => setPaused((p) => !p)} aria-pressed={paused}>
            {paused ? <Play /> : <Pause />}
            {paused ? "Resume" : "Pause"}
          </Button>
          <Button variant="ghost-muted" size="icon-sm" aria-label="Refresh now" onClick={load}>
            <RefreshCw />
          </Button>
        </div>
      </PageHeader>

      {(error || message || activity?.limitedView) && (
        <div className="flex shrink-0 flex-col gap-1 border-b border-border px-4 py-2 text-[13px]">
          {error && <span className="text-destructive-foreground">{error}</span>}
          {message && (
            <button type="button" className="text-left text-muted-foreground cursor-pointer" onClick={() => setMessage(null)}>
              {message}
            </button>
          )}
          {activity?.limitedView && (
            <span className="flex items-center gap-1.5 text-warning-foreground">
              <TriangleAlert size={13} />
              Your role sees only its own queries; others show as hidden (needs pg_read_all_stats).
            </span>
          )}
        </div>
      )}

      {!activity && !error ? (
        <TableSkeleton columns={6} rows={8} />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 z-[1] select-none">
              <tr className="text-[13px] text-muted-foreground [&>th]:border-b [&>th]:border-border [&>th]:bg-card-header [&>th]:px-3 [&>th]:py-2.5 [&>th]:text-left [&>th]:font-medium [&>th]:whitespace-nowrap">
                <th>PID</th>
                <th>State</th>
                <th>Query</th>
                <th>Running for</th>
                <th>User</th>
                {scope === "all" && <th>Database</th>}
                <th>Client</th>
                <th className="w-px" />
              </tr>
            </thead>
            <tbody>
              {sessions.map((s, i) => (
                <tr key={s.pid} className={cn("align-top border-b border-border", i % 2 ? "bg-muted" : "bg-card", s.blockedBy.length > 0 && "bg-destructive/5")}>
                  <td className="px-3 py-2 font-mono text-xs tabular-nums">
                    {s.pid}
                    {s.isSelf && <div className="text-[10px] text-muted-foreground">DBunny</div>}
                  </td>
                  <td className="px-3 py-2">
                    <div className="flex flex-col items-start gap-1">
                      <Badge variant={STATE_BADGE[s.state ?? ""] ?? "neutral"}>{s.state ?? "hidden"}</Badge>
                      {blocking.has(s.pid) && <Badge variant="destructive">blocking others</Badge>}
                      {s.blockedBy.length > 0 && <span className="text-xs text-destructive-foreground">waits for {s.blockedBy.join(", ")}</span>}
                      {s.waitingFor && s.blockedBy.length === 0 && <span className="text-xs text-muted-foreground">{s.waitingFor}</span>}
                    </div>
                  </td>
                  <td className="max-w-[28rem] px-3 py-2">
                    <button
                      type="button"
                      onClick={() => setExpanded(expanded === s.pid ? null : s.pid)}
                      className={cn("block w-full text-left font-mono text-xs cursor-pointer break-words whitespace-pre-wrap", expanded !== s.pid && "line-clamp-2")}
                    >
                      {s.query || <span className="text-muted-foreground">—</span>}
                    </button>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 tabular-nums">
                    {s.state === "active" ? duration(s.querySeconds) : <span className="text-muted-foreground">{duration(s.querySeconds)} ago</span>}
                    {s.transactionSeconds !== null && s.state !== "active" && (
                      <div className="text-xs text-warning-foreground">transaction {duration(s.transactionSeconds)}</div>
                    )}
                  </td>
                  <td className="px-3 py-2">{s.user ?? "—"}</td>
                  {scope === "all" && <td className="px-3 py-2">{s.database ?? "—"}</td>}
                  <td className="px-3 py-2 text-xs text-muted-foreground">
                    {s.application && <div className="text-foreground">{s.application}</div>}
                    {s.client ?? "local"}
                  </td>
                  <td className="whitespace-nowrap px-3 py-2">
                    {!s.isSelf && !readOnly && (
                      <div className="flex gap-1">
                        <Button variant="ghost-muted" size="xs" disabled={s.state !== "active"} onClick={() => setConfirm({ session: s, terminate: false })}>
                          <Square className="size-3" /> Cancel
                        </Button>
                        <Button variant="destructive-ghost" size="xs" onClick={() => setConfirm({ session: s, terminate: true })}>
                          <XCircle className="size-3" /> End
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {sessions.length === 0 && <p className="px-4 py-10 text-center text-sm text-muted-foreground">No sessions.</p>}
        </div>
      )}

      <ConfirmDialog
        open={confirm !== null}
        onClose={() => setConfirm(null)}
        onConfirm={() => confirm && signal(confirm.session, confirm.terminate)}
        icon={confirm?.terminate ? <XCircle /> : <Square />}
        tone={confirm?.terminate ? "destructive" : "warning"}
        title={confirm?.terminate ? `End session ${confirm.session.pid}?` : `Cancel the query of ${confirm?.session.pid}?`}
        description={
          confirm?.terminate
            ? "Disconnects it from the server. An open transaction is rolled back."
            : "Stops the running statement; the session stays connected and gets an error."
        }
        confirmLabel={confirm?.terminate ? "End session" : "Cancel query"}
        cancelLabel="Keep it"
      />
    </div>
  );
}
