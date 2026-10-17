import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Activity, Copy, Pause, Play, RefreshCw, Square, TriangleAlert, X, XCircle } from "lucide-react";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu";
import { writeClipboardText } from "../lib/clipboard";
import type { ServerActivity, ServerSession } from "@/types";
import { useConnection } from "../contexts/useConnection";
import { useNotify } from "../contexts/NoticeProvider";
import { useTab } from "../contexts/workspace";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { TableSkeleton } from "../components/Skeleton";
import SqlPreview from "../components/schema/SqlPreview";
import Badge from "../components/ui/Badge";
import Button from "../components/ui/Button";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import { useRightPanel } from "../contexts/rightPanel";
import SideCard from "../components/layout/SideCard";
import { useSqlDialect } from "../hooks/useSqlDialect";
import { cn } from "../lib/cn";
import { formatSqlCached } from "../lib/formatSql";

const REFRESH_MS = 3000;

function duration(seconds: number | null) {
  if (seconds === null) return "—";
  if (seconds < 1) return `${Math.round(seconds * 1000)} ms`;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ${Math.floor(seconds % 60)} s`;
  return `${Math.floor(seconds / 3600)} h ${Math.floor((seconds % 3600) / 60)} min`;
}

/** Short clock-style time for the table, so the column stays narrow. */
function durationCompact(seconds: number | null) {
  if (seconds === null) return "—";
  const s = Math.max(0, seconds);
  if (s < 1) return `${Math.round(s * 1000)}ms`;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  const sec = Math.floor(s % 60);
  const min = Math.floor(s / 60);
  if (s < 3600) return `${min}:${String(sec).padStart(2, "0")}`;
  const hour = Math.floor(min / 60);
  return `${hour}:${String(min % 60).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

const STATE_LABEL: Record<string, string> = {
  active: "active",
  idle: "idle",
  "idle in transaction": "idle tx",
  "idle in transaction (aborted)": "aborted",
};

function oneLine(sql: string) {
  return sql.replace(/\s+/g, " ").trim();
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
  const notify = useNotify();
  const readOnly = connectionInfo?.readOnly ?? false;
  const [scope, setScope] = useState<"database" | "all">("database");
  const [activity, setActivity] = useState<ServerActivity | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reportedError = useRef<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [confirm, setConfirm] = useState<{ session: ServerSession; terminate: boolean } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRefreshing(true);
      setActivity(await api.getServerActivity(scope === "all"));
      setError(null);
      reportedError.current = null;
      setLastRefresh(new Date());
    } catch (err) {
      const text = typeof err === "string" ? err : "Couldn't read the server activity";
      if (reportedError.current !== text) {
        reportedError.current = text;
        notify({ tone: "error", text });
      }
      setError(text);
    } finally {
      setRefreshing(false);
    }
  }, [api, notify, scope]);

  useEffect(() => {
    const first = setTimeout(load, 0);
    return () => clearTimeout(first);
  }, [load]);

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
      notify({ tone: "error", text: typeof err === "string" ? err : "It didn't work" });
    }
    load();
  }

  const rightPanel = useRightPanel();
  const sessions = activity?.sessions ?? [];
  const blocking = useMemo(() => new Set(sessions.flatMap((s) => s.blockedBy)), [sessions]);
  const selectedSession = sessions.find((s) => s.pid === selected) ?? null;
  // Last session shown, so the card still has content while it slides shut.
  const [shown, setShown] = useState<ServerSession | null>(null);

  useEffect(() => {
    if (selectedSession) setShown(selectedSession);
  }, [selectedSession]);

  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (paused || !lastRefresh) return;
    const timer = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, [paused, lastRefresh]);

  const statusLine = useMemo(() => {
    if (!lastRefresh) return null;
    void tick;
    const sec = Math.max(0, Math.floor((Date.now() - lastRefresh.getTime()) / 1000));
    if (sec < 5) return "just now";
    if (sec < 60) return `${sec}s ago`;
    return `${Math.floor(sec / 60)}m ago`;
  }, [lastRefresh, tick]);

  return (
    <div className="flex h-full flex-col">
      <PageHeader className="justify-between gap-2">
        <HeaderTitle label="Server" icon={<Activity />}>
          Activity
        </HeaderTitle>
        <div className="flex flex-wrap items-center justify-end gap-2">
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
          <Button variant="ghost-muted" size="icon-sm" aria-label="Refresh now" onClick={load} disabled={refreshing}>
            <RefreshCw className={cn(refreshing && "animate-spin")} />
          </Button>
        </div>
      </PageHeader>

      {(message || activity?.limitedView) && (
        <div className="flex shrink-0 flex-col gap-1 border-b border-border px-4 py-2 text-[13px]">
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
      ) : !activity ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-muted-foreground">Couldn't read the server activity.</div>
      ) : (
        <>
          <div className="min-h-0 flex-1 overflow-auto">
              <table className="w-full table-fixed text-sm">
                <colgroup>
                  <col className="w-[5.5rem]" />
                  <col className="w-[6.25rem]" />
                  <col />
                  <col className="w-[6.25rem]" />
                  <col className="w-[6.5rem]" />
                  {scope === "all" && <col className="w-[7rem]" />}
                  <col className="w-[9rem]" />
                  <col className="w-[11.25rem]" />
                </colgroup>
                <thead className="sticky top-0 z-[1] select-none">
                  <tr className="text-[13px] text-muted-foreground [&>th]:border-b [&>th]:border-border [&>th]:bg-card-header [&>th]:px-4 [&>th]:py-2.5 [&>th]:text-left [&>th]:font-medium [&>th]:whitespace-nowrap">
                    <th>PID</th>
                    <th>State</th>
                    <th>Query</th>
                    <th>Duration</th>
                    <th>User</th>
                    {scope === "all" && <th>Database</th>}
                    <th>Client</th>
                    <th className="px-2" aria-hidden />
                  </tr>
                </thead>
                <tbody>
                  {sessions.map((s, i) => {
                    const activeRow = selected === s.pid;
                    return (
                      <tr
                        key={s.pid}
                        onClick={() => setSelected(activeRow ? null : s.pid)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          const items: ContextMenuItem[] = [];
                          if (s.query) items.push({ label: "Copy query", icon: <Copy />, onSelect: () => writeClipboardText(s.query ?? "") });
                          if (!s.isSelf && !readOnly) {
                            if (items.length) items.push("separator");
                            items.push(
                              { label: "Cancel query", icon: <Square />, disabled: s.state !== "active", onSelect: () => setConfirm({ session: s, terminate: false }) },
                              { label: "End session", icon: <XCircle />, destructive: true, onSelect: () => setConfirm({ session: s, terminate: true }) },
                            );
                          }
                          if (items.length) setMenu({ x: e.clientX, y: e.clientY, items });
                        }}
                        className={cn(
                          "cursor-pointer border-b border-border align-middle transition-colors hover:bg-accent",
                          activeRow ? "bg-accent" : i % 2 ? "bg-muted/50" : "bg-card",
                          !activeRow && s.blockedBy.length > 0 && "bg-destructive/[0.04]",
                        )}
                      >
                        <td className="px-4 py-2 font-mono text-xs tabular-nums whitespace-nowrap">
                          {s.pid}
                          {s.isSelf && <span className="ml-1.5 font-sans text-[11px] font-normal text-primary">you</span>}
                        </td>
                        <td className="px-3 py-2">
                          <div className="flex items-center gap-1.5">
                            <Badge variant={STATE_BADGE[s.state ?? ""] ?? "neutral"}>
                              {STATE_LABEL[s.state ?? ""] ?? s.state ?? "hidden"}
                            </Badge>
                            {blocking.has(s.pid) && <span className="size-1.5 shrink-0 rounded-full bg-destructive" />}
                          </div>
                        </td>
                        <td className="px-4 py-2">
                          <p className="truncate font-mono text-xs text-muted-foreground">
                            {s.query ? oneLine(s.query) : "—"}
                          </p>
                        </td>
                        <td
                          className={cn(
                            "whitespace-nowrap px-4 py-2 text-right tabular-nums text-[13px]",
                            s.state !== "active" && "text-muted-foreground",
                          )}
                        >
                          {durationCompact(s.querySeconds)}
                        </td>
                        <td className="truncate px-4 py-2">{s.user ?? "—"}</td>
                        {scope === "all" && <td className="truncate px-4 py-2">{s.database ?? "—"}</td>}
                        <td className="truncate px-4 py-2 text-xs text-muted-foreground">
                          {s.application ?? s.client ?? "local"}
                        </td>
                        <td className="px-2 py-1.5">
                          {!s.isSelf && !readOnly && (
                            <div className="flex flex-nowrap items-center justify-end gap-1">
                              <Button
                                variant="ghost-muted"
                                size="xs"
                                className="shrink-0 whitespace-nowrap"
                                disabled={s.state !== "active"}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  setConfirm({ session: s, terminate: false });
                                }}
                              >
                                <Square className="size-3" /> Cancel
                              </Button>
                              <Button
                                variant="destructive-ghost"
                                size="xs"
                                className="shrink-0 whitespace-nowrap"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  setConfirm({ session: s, terminate: true });
                                }}
                              >
                                <XCircle className="size-3" /> End
                              </Button>
                            </div>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
              {sessions.length === 0 && <p className="px-4 py-10 text-center text-sm text-muted-foreground">No sessions.</p>}
          </div>

          {rightPanel && createPortal(
            <SideCard open={selected !== null && shown !== null} widthClass="w-96" label={shown ? `Session ${shown.pid}` : "Session"}>
              {shown && (
                <SessionPanel
                  session={selectedSession ?? shown}
                  isBlocking={blocking.has((selectedSession ?? shown).pid)}
                  showDatabase={scope === "all"}
                  readOnly={readOnly}
                  onClose={() => setSelected(null)}
                  onCancel={() => selectedSession && setConfirm({ session: selectedSession, terminate: false })}
                  onEnd={() => selectedSession && setConfirm({ session: selectedSession, terminate: true })}
                />
              )}
            </SideCard>,
            rightPanel,
          )}

          <footer className="flex h-9 shrink-0 items-center justify-between gap-3 border-t border-border bg-card-header px-4 text-[12px] text-muted-foreground">
            <button type="button" onClick={load} disabled={refreshing} className="flex items-center gap-1.5 cursor-pointer hover:text-foreground disabled:opacity-50">
              <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
              {sessions.length} session{sessions.length === 1 ? "" : "s"}
              {statusLine && <span className="text-muted-foreground/80">· updated {statusLine}</span>}
            </button>
            <span>{paused ? "Updates paused" : `Auto-refresh every ${REFRESH_MS / 1000}s`}</span>
          </footer>
        </>
      )}

      <ContextMenu position={menu} items={menu?.items ?? []} onClose={() => setMenu(null)} />
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

function SessionPanel({
  session,
  isBlocking,
  showDatabase,
  readOnly,
  onClose,
  onCancel,
  onEnd,
}: {
  session: ServerSession;
  isBlocking: boolean;
  showDatabase: boolean;
  readOnly: boolean;
  onClose: () => void;
  onCancel: () => void;
  onEnd: () => void;
}) {
  const dialect = useSqlDialect();
  const [sql, setSql] = useState(session.query ?? "");

  useEffect(() => {
    if (!session.query) {
      setSql("");
      return;
    }
    let cancelled = false;
    setSql(session.query);
    void formatSqlCached(session.query, dialect).then((text) => {
      if (!cancelled) setSql(text);
    });
    return () => {
      cancelled = true;
    };
  }, [session.query, dialect]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="font-mono text-sm tabular-nums">{session.pid}</span>
          {session.isSelf && <span className="text-[11px] text-primary">you</span>}
          <Badge variant={STATE_BADGE[session.state ?? ""] ?? "neutral"}>{session.state ?? "hidden"}</Badge>
          {isBlocking && <Badge variant="destructive">blocking</Badge>}
        </div>
        <Button variant="ghost-muted" size="icon-sm" aria-label="Close" onClick={onClose}>
          <X />
        </Button>
      </header>

      <dl className="grid shrink-0 grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 border-b border-border px-3 py-2.5 text-[12px]">
        <Field label="User" value={session.user ?? "—"} />
        <Field label="Duration" value={session.state === "active" ? duration(session.querySeconds) : `${duration(session.querySeconds)} ago`} />
        {session.transactionSeconds !== null && <Field label="Transaction" value={duration(session.transactionSeconds)} />}
        {showDatabase && <Field label="Database" value={session.database ?? "—"} />}
        <Field label="Application" value={session.application ?? "—"} />
        <Field label="Client" value={session.client ?? "local"} />
        {session.waitingFor && <Field label="Waiting" value={session.waitingFor} />}
        {session.blockedBy.length > 0 && <Field label="Blocked by" value={session.blockedBy.join(", ")} />}
      </dl>

      <div className="min-h-0 flex-1 overflow-auto">
        {sql ? (
          <div className="px-3 py-2 font-mono text-xs">
            <SqlPreview source={sql} />
          </div>
        ) : (
          <p className="px-3 py-6 text-sm text-muted-foreground">No query.</p>
        )}
      </div>

      {!session.isSelf && !readOnly && (
        <footer className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-3 py-2">
          <Button variant="ghost-muted" size="sm" disabled={session.state !== "active"} onClick={onCancel}>
            <Square /> Cancel query
          </Button>
          <Button variant="destructive-ghost" size="sm" onClick={onEnd}>
            <XCircle /> End session
          </Button>
        </footer>
      )}
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="select-text truncate text-foreground">
        {value}
      </dd>
    </>
  );
}
