import { useState, useCallback, useRef, useEffect } from "react";
import { useConnection } from "../../contexts/useConnection";
import { useNotify } from "../../contexts/NoticeProvider";
import { useDocuments } from "../../contexts/DocumentsContext";
import type { CompletionSchema, ExplainResult, QueryResult } from "@/types";
import QueryResults from "../QueryResults";
import SqlEditor, { type SqlEditorHandle } from "../SqlEditor";
import { Upload, Download, ShieldCheck, ShieldOff, AlertTriangle, History, Check, MoreHorizontal, AlignLeft, Save } from "lucide-react";
import RunButton from "../query/RunButton";
import TransactionButton from "../query/TransactionButton";
import ResultsBar from "../query/ResultsBar";
import Tooltip from "../Tooltip";
import { createPortal } from "react-dom";
import { useRightPanel } from "../../contexts/rightPanel";
import type { RunScope } from "../SqlEditor";
import SegmentedTabs from "../ui/SegmentedTabs";
import Popover from "../ui/Popover";
import PlanView from "../query/PlanView";
import NoticeList from "../query/NoticeList";
import HistoryPanel, { type LibraryView } from "../query/HistoryPanel";
import SideCard from "../layout/SideCard";
import { registerEditor } from "../../lib/editorBridge";
import { useAssistant } from "../../contexts/assistant";
import { errorTextOf, explainPrompt, fixPrompt, optimizePrompt } from "../../lib/assistantPrompts";
import { engineLabel } from "../../lib/engines";
import AssistantActions from "../query/AssistantActions";
import Dialog from "../ui/Dialog";
import { Keys } from "../ui/Kbd";
import Input from "../ui/Input";
import { loadSaved, removeSaved, saveQuery, savedKey, type SavedQuery } from "../../lib/savedQueries";
import { addToHistory, clearHistory, errorSummary, historyKey, loadHistory, removeFromHistory, type HistoryEntry } from "../../lib/queryHistory";
import Button from "../ui/Button";
import ConfirmDialog from "../ui/ConfirmDialog";
import { isMac } from "../../lib/platform";
import { getSettings } from "../../lib/settings";
import { explainAnalyzeHint, notAvailableFor } from "../../lib/engines";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useT, t } from "../../i18n";

const MOD = isMac ? "⌘" : "Ctrl+";
const SHIFT = isMac ? "⇧" : "Shift+";
const ALT = isMac ? "⌥" : "Alt+";

type ResultView = "results" | "messages" | "plan";

/** Plain transaction statements typed in the editor are routed to the transaction controls. */
function transactionCommand(sql: string): "begin" | "commit" | "rollback" | null {
  const statement = sql.replace(/--[^\n]*/g, " ").trim().replace(/;$/, "").trim().toUpperCase().replace(/\s+/g, " ");
  if (statement === "BEGIN" || statement === "BEGIN TRANSACTION" || statement === "BEGIN WORK" || statement === "START TRANSACTION") return "begin";
  if (statement === "COMMIT" || statement === "END" || statement === "COMMIT TRANSACTION" || statement === "COMMIT WORK") return "commit";
  if (statement === "ROLLBACK" || statement === "ABORT" || statement === "ROLLBACK TRANSACTION" || statement === "ROLLBACK WORK") return "rollback";
  return null;
}

type DangerInfo = {
  rule: string;
  detail: string;
};

// Statements that can add/remove tables, columns or functions
const DDL_PATTERN = /\b(CREATE|ALTER|DROP|RENAME)\b/i;

function detectDangerousStatements(sql: string): DangerInfo[] {
  const dangers: DangerInfo[] = [];

  const normalized = sql
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/'(?:[^'\\]|\\.)*'/g, "''")
    .replace(/\s+/g, " ")
    .trim()
    .toUpperCase();

  const stmts = normalized.split(";").map((s) => s.trim()).filter(Boolean);

  for (const stmt of stmts) {
    if (/\bDROP\s+DATABASE\b/.test(stmt)) {
      dangers.push({ rule: "DROP DATABASE", detail: t("ui.queryPane.dangerDropDatabase") });
    } else if (/\bDROP\s+SCHEMA\b/.test(stmt)) {
      dangers.push({ rule: "DROP SCHEMA", detail: t("ui.queryPane.dangerDropSchema") });
    } else if (/\bDROP\s+TABLE\b/.test(stmt)) {
      dangers.push({ rule: "DROP TABLE", detail: t("ui.queryPane.dangerDropTable") });
    } else if (/\bTRUNCATE\b/.test(stmt)) {
      dangers.push({ rule: "TRUNCATE", detail: t("ui.queryPane.dangerTruncate") });
    } else if (/\bDELETE\b/.test(stmt) && !/\bWHERE\b/.test(stmt)) {
      dangers.push({ rule: "DELETE without WHERE", detail: t("ui.queryPane.dangerDelete") });
    } else if (/\bUPDATE\b/.test(stmt) && /\bSET\b/.test(stmt) && !/\bWHERE\b/.test(stmt)) {
      dangers.push({ rule: "UPDATE without WHERE", detail: t("ui.queryPane.dangerUpdate") });
    } else if (/\bALTER\s+TABLE\b/.test(stmt) && /\bDROP\s+COLUMN\b/.test(stmt)) {
      dangers.push({ rule: "ALTER TABLE DROP COLUMN", detail: t("ui.queryPane.dangerDropColumn") });
    }
  }

  return dangers;
}

/** One query. The connection's document list owns the tabs; this pane is a single editor. */
export default function QueryPane({
  documentId,
  initialSql,
  savedId: savedIdProp,
  title,
  active,
}: {
  documentId: string;
  initialSql: string;
  savedId?: string;
  title: string;
  active: boolean;
}) {
  const t = useT();
  const { connectionInfo } = useConnection();
  const capabilities = useCapabilities();
  if (!capabilities.console) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 p-6 text-center">
        <p className="text-[13px] font-medium">{t("ui.queryPane.sqlEditor")}</p>
        <p className="text-xs text-muted-foreground">{connectionInfo ? notAvailableFor(connectionInfo.engine) : ""}</p>
      </div>
    );
  }
  return <QueryPaneBody documentId={documentId} initialSql={initialSql} savedId={savedIdProp} title={title} active={active} />;
}

function QueryPaneBody({
  documentId,
  initialSql,
  savedId: savedIdProp,
  title,
  active,
}: {
  documentId: string;
  initialSql: string;
  savedId?: string;
  title: string;
  active: boolean;
}) {
  const t = useT();
  const { openDocument, setQueryDirty, setQuerySaved } = useDocuments();
  const rightPanel = useRightPanel();
  const renderSidePanel = (panel: React.ReactNode) => (rightPanel ? createPortal(panel, rightPanel) : null);
  const { api, connectionInfo } = useConnection();
  const capabilities = useCapabilities();
  const historyStorageKey = connectionInfo ? historyKey(connectionInfo) : null;
  const [history, setHistory] = useState<HistoryEntry[]>(() => (historyStorageKey ? loadHistory(historyStorageKey) : []));
  const [historyOpen, setHistoryOpen] = useState(false);
  const [libraryView, setLibraryView] = useState<LibraryView>("history");
  const savedStorageKey = connectionInfo ? savedKey(connectionInfo) : null;
  const [savedQueries, setSavedQueries] = useState<SavedQuery[]>(() => (savedStorageKey ? loadSaved(savedStorageKey) : []));
  // Name being typed when saving a query for the first time
  const [saveName, setSaveName] = useState<string | null>(null);
  const [justSaved, setJustSaved] = useState(false);
  const notify = useNotify();
  const showFormatError = useCallback((message: string) => {
    notify({ tone: "error", text: t("ui.queryPane.formatFailed", { message }) });
  }, [notify, t]);
  const [linkedSavedId, setLinkedSavedId] = useState(savedIdProp);
  // Share of the height the editor takes over the results, remembered across sessions
  const [split, setSplit] = useState(() => {
    try {
      const n = Number(localStorage.getItem("dbunny:editorSplit"));
      return n >= 20 && n <= 80 ? n : 50;
    } catch {
      return 50;
    }
  });
  const columnRef = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [plan, setPlan] = useState<ExplainResult | null>(null);
  const [view, setView] = useState<ResultView>("results");
  const [inTransaction, setInTransaction] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  // When the running query started, for the elapsed time under the results
  const [runningSince, setRunningSince] = useState<number | null>(null);
  const [moreOpen, setMoreOpen] = useState(false);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const [safeMode, setSafeMode] = useState(() => getSettings().safeModeByDefault);
  const [pendingDangers, setPendingDangers] = useState<DangerInfo[] | null>(null);
  const pendingSqlRef = useRef<string | null>(null);
  const [completionSchema, setCompletionSchema] = useState<CompletionSchema | null>(null);

  const editorRef = useRef<SqlEditorHandle | null>(null);
  const assistant = useAssistant();
  // The statement that failed, and the plan the person last asked for, to hand to the assistant
  const lastRunRef = useRef("");
  const lastPlanRef = useRef<{ sql: string; text: string } | null>(null);

  const loadCompletionSchema = useCallback(() => {
    api
      .getCompletionSchema()
      .then((schema) => setCompletionSchema(schema))
      .catch(() => {});
  }, [api]);

  useEffect(() => {
    loadCompletionSchema();
  }, [loadCompletionSchema]);

  // The transaction lives on the server side of the connection; ask whether one is open
  useEffect(() => {
    if (capabilities.transactions) api.transactionOpen().then(setInTransaction).catch(() => {});
  }, [api, capabilities.transactions]);

  // A double-click rename of a saved query updates the stored name too.
  useEffect(() => {
    if (!linkedSavedId || !savedStorageKey) return;
    const stored = savedQueries.find((query) => query.id === linkedSavedId);
    if (!stored || stored.name === title) return;
    setSavedQueries(saveQuery(savedStorageKey, { id: stored.id, name: title, sql: stored.sql }).list);
  }, [title, linkedSavedId, savedStorageKey, savedQueries]);

  // Read from the editor's change handler, which is created once
  const latest = useRef({ linkedSavedId, savedQueries, initialSql, documentId });
  useEffect(() => {
    latest.current = { linkedSavedId, savedQueries, initialSql, documentId };
  });

  const handleEditorChange = useCallback((sql: string) => {
    const { linkedSavedId: savedId, savedQueries: list, initialSql: initial, documentId: id } = latest.current;
    if (savedId) {
      const stored = list.find((query) => query.id === savedId);
      setQueryDirty(id, stored !== undefined && stored.sql !== sql);
    } else {
      setQueryDirty(id, sql !== initial);
    }
  }, [setQueryDirty]);

  function startResize(e: React.PointerEvent) {
    const column = columnRef.current;
    if (!column) return;
    e.preventDefault();
    const rect = column.getBoundingClientRect();
    let value = split;
    const onMove = (ev: PointerEvent) => {
      value = Math.min(80, Math.max(20, ((ev.clientY - rect.top) / rect.height) * 100));
      setSplit(value);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      try {
        localStorage.setItem("dbunny:editorSplit", String(Math.round(value)));
      } catch {
        // only for this session
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  /** Opens SQL from the history as its own document, leaving this one as it is. */
  function openFromHistory(sql: string, options: { label?: string; savedId?: string } = {}) {
    if (options.savedId) openDocument({ kind: "query", sql, savedId: options.savedId, title: options.label });
    else openDocument({ kind: "query", sql });
  }

  /** Saves this query: updates its saved query, or asks for a name the first time. */
  function saveActiveQuery(name?: string) {
    if (!savedStorageKey) return;
    const sql = editorRef.current?.getValue() ?? "";
    const existing = linkedSavedId ? savedQueries.find((query) => query.id === linkedSavedId) : undefined;
    if (!existing && name === undefined) {
      setSaveName(/^Query \d+$/.test(title) ? "" : title);
      return;
    }
    const { list, saved: stored } = saveQuery(savedStorageKey, { id: existing?.id, name: name ?? existing!.name, sql });
    setSavedQueries(list);
    setLinkedSavedId(stored.id);
    setQuerySaved(documentId, stored.id, stored.name);
    setQueryDirty(documentId, false);
    setSaveName(null);
    setJustSaved(true);
    setTimeout(() => setJustSaved(false), 1500);
  }

  async function exportQuery() {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    const sql = editorRef.current?.getValue() ?? initialSql;
    const defaultName = `${title.replace(/\s+/g, "_").toLowerCase()}.sql`;
    const path = await save({
      defaultPath: defaultName,
      filters: [{ name: "SQL", extensions: ["sql"] }, { name: "Text", extensions: ["txt"] }],
    });
    if (path) await writeTextFile(path, sql);
  }

  async function importQuery() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { readTextFile } = await import("@tauri-apps/plugin-fs");
    const path = await open({
      multiple: false,
      filters: [{ name: "SQL", extensions: ["sql"] }, { name: "Text", extensions: ["txt"] }],
    });
    if (path && typeof path === "string") {
      const sql = await readTextFile(path);
      editorRef.current?.setValue(sql);
      handleEditorChange(sql);
    }
  }

  const showView = useCallback((next: ResultView) => {
    setView(next);
  }, []);

  const runTransaction = useCallback(async (command: "begin" | "commit" | "rollback") => {
    setTxBusy(true);
    setError(null);
    try {
      if (command === "begin") await api.beginTransaction();
      else if (command === "commit") await api.commitTransaction();
      else await api.rollbackTransaction();
    } catch (err) {
      notify({ tone: "error", text: typeof err === "string" ? err : err instanceof Error ? err.message : t("ui.queryPane.transactionFailed") });
    } finally {
      // Commit and rollback close the transaction even when they fail
      setInTransaction(await api.transactionOpen().catch(() => false));
      setTxBusy(false);
    }
  }, [api, notify, t]);

  const executeQuery = useCallback(async (sql: string) => {
    const txCommand = transactionCommand(sql);
    if (txCommand) {
      await runTransaction(txCommand);
      return;
    }
    lastRunRef.current = sql;
    setRunning(true);
    setRunningSince(Date.now());
    setError(null);
    setResult(null);
    const delay = new Promise<void>((r) => setTimeout(r, 400));
    try {
      const [res] = await Promise.all([api.query(sql), delay]);
      setResult(res);
      if (historyStorageKey) {
        setHistory(addToHistory(historyStorageKey, {
          sql,
          durationMs: res.duration,
          rows: res.rowsAffected ?? res.rowCount,
          error: null,
        }));
      }
      // A script may have started or ended a transaction (BEGIN IMMEDIATE, COMMIT...): the toolbar follows
      if (capabilities.transactions) api.transactionOpen().then(setInTransaction).catch(() => {});
      // A script that only printed messages opens on them
      showView(res.columns.length === 0 && (res.notices?.length ?? 0) > 0 ? "messages" : "results");
      if (DDL_PATTERN.test(sql)) loadCompletionSchema();
    } catch (err) {
      await delay;
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : t("ui.queryPane.queryFailed");
      setError(msg);
      if (historyStorageKey) {
        setHistory(addToHistory(historyStorageKey, { sql, durationMs: null, rows: null, error: errorSummary(msg) }));
      }
      // A lost transaction connection ends the transaction, and a script that failed after its BEGIN
      // leaves one open (SQLite keeps the lock): the toolbar follows what the connection says
      if (inTransaction || capabilities.transactions) api.transactionOpen().then(setInTransaction).catch(() => setInTransaction(false));
      showView("results");
    } finally {
      setRunning(false);
      setRunningSince(null);
    }
  }, [loadCompletionSchema, api, runTransaction, showView, historyStorageKey, inTransaction, capabilities.transactions, t]);

  const explain = useCallback(async (analyze: boolean) => {
    // EXPLAIN takes one statement: the selection, or the one at the cursor
    const sql = editorRef.current?.getCurrentStatement() ?? "";
    if (!sql.trim()) return;
    setRunning(true);
    setRunningSince(Date.now());
    setError(null);
    try {
      const res = await api.explain(sql, analyze);
      lastPlanRef.current = { sql, text: JSON.stringify(res.plan).slice(0, 6000) };
      setPlan(res);
      showView("plan");
    } catch (err) {
      notify({ tone: "error", text: typeof err === "string" ? err : err instanceof Error ? err.message : t("ui.queryPane.explainFailed") });
    } finally {
      setRunning(false);
      setRunningSince(null);
    }
  }, [api, notify, t]);

  // What the assistant is asked about: the selection, or the statement at the cursor
  const engine = connectionInfo ? engineLabel(connectionInfo.engine) : "SQL";
  const askAbout = useCallback(
    (build: (sql: string) => string) => {
      const sql = editorRef.current?.getCurrentStatement() ?? "";
      if (!sql.trim()) {
        notify({ tone: "error", text: t("ui.queryPane.selectAStatementOrPut") });
        return;
      }
      assistant.ask(build(sql));
    },
    [assistant, notify, t],
  );
  const explainWithAssistant = useCallback(() => askAbout((sql) => explainPrompt({ engine, sql })), [askAbout, engine]);
  const optimizeWithAssistant = useCallback(
    () =>
      askAbout((sql) => {
        const plan = lastPlanRef.current;
        return optimizePrompt({ engine, sql, plan: plan && plan.sql === sql ? plan.text : undefined });
      }),
    [askAbout, engine],
  );
  const fixWithAssistant = useCallback(() => {
    if (error && lastRunRef.current) assistant.ask(fixPrompt({ engine, sql: lastRunRef.current, error: errorTextOf(error) }));
  }, [assistant, engine, error]);

  const runQuery = useCallback(async (scope: RunScope = "current") => {
    const editor = editorRef.current;
    // A run waits out a transaction opening or closing (a blink, so Run isn't greyed out for it)
    if (!editor || txBusy) return;
    const sql = scope === "all" ? editor.getValue() : editor.getCurrentStatement();
    if (!sql.trim()) return;

    if (safeMode) {
      const dangers = detectDangerousStatements(sql);
      if (dangers.length > 0) {
        pendingSqlRef.current = sql;
        setPendingDangers(dangers);
        return;
      }
    }

    await executeQuery(sql);
  }, [safeMode, executeQuery, txBusy]);

  const cancelQuery = useCallback(() => {
    api.cancelQuery().catch(() => {});
  }, [api]);

  // Cmd+S (Ctrl+S off the Mac) saves the query: asks for a name the first time, then updates it.
  // Only the visible tab listens, and not while a dialog is open (the name dialog has its own Enter).
  const saveRef = useRef(saveActiveQuery);
  useEffect(() => {
    saveRef.current = saveActiveQuery;
  });
  useEffect(() => {
    if (!active) return;
    function onKeyDown(e: KeyboardEvent) {
      const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!mod || e.shiftKey || e.altKey || e.key.toLowerCase() !== "s") return;
      e.preventDefault();
      if (document.querySelector("[role=dialog][aria-modal]")) return;
      saveRef.current();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active]);

  // The assistant writes into the editor of the tab in front
  useEffect(() => {
    const handle = editorRef.current;
    if (!active || !handle) return;
    return registerEditor(documentId, handle);
  }, [active, documentId]);

  // Esc stops the running query (only in the visible tab)
  useEffect(() => {
    if (!running || !active) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && !document.querySelector("[role=dialog][aria-modal]")) cancelQuery();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [running, active, cancelQuery]);

  function confirmDangerousQuery() {
    const sql = pendingSqlRef.current;
    setPendingDangers(null);
    pendingSqlRef.current = null;
    if (sql) executeQuery(sql);
  }

  function cancelDangerousQuery() {
    setPendingDangers(null);
    pendingSqlRef.current = null;
  }

  return (
    <div className="h-full flex">
      <div ref={columnRef} className="h-full flex flex-col flex-1 min-w-0">
        <ConfirmDialog
          open={pendingDangers !== null}
          onClose={cancelDangerousQuery}
          onConfirm={confirmDangerousQuery}
          icon={<AlertTriangle />}
          tone="destructive"
          title={t("ui.queryPane.blockedBySafeMode")}
          description={
            t("ui.queryPane.dangersDetected", { count: pendingDangers?.length ?? 0 })
          }
          cancelLabel={t("common.cancel")}
          confirmLabel={t("ui.queryPane.runAnyway")}
        >
          <div className="rounded-xl border border-border bg-muted divide-y divide-border">
            {(pendingDangers ?? []).map((d, i) => (
              <div key={i} className="px-3 py-2.5">
                <span className="block select-text font-mono text-xs font-medium text-destructive-foreground">{d.rule}</span>
                <span className="text-xs text-muted-foreground block mt-0.5">{d.detail}</span>
              </div>
            ))}
          </div>
        </ConfirmDialog>

        {/* Editor panel: its height is dragged with the handle under it */}
        <div className="flex min-h-0 shrink-0 grow-0 flex-col border-b border-border" style={{ flexBasis: `${split}%` }}>
          {/* SQL editor */}
          <div className="flex-1 min-h-0">
            <SqlEditor
              onMount={(handle) => { editorRef.current = handle; }}
              initialValue={initialSql}
              schema={completionSchema}
              onRun={runQuery}
              onChange={handleEditorChange}
              onFormatError={showFormatError}
            />
          </div>

          {/* Actions: run and save on the left, the rest quieter on the right */}
          <div className="flex h-11 shrink-0 items-center gap-2 border-t border-border px-2.5 select-none">
            <RunButton disabled={running} explainAnalyze={capabilities.explainAnalyze} analyzeHint={explainAnalyzeHint(capabilities.explainFormat)} onRun={runQuery} onExplain={explain} />
            <AssistantActions onExplain={explainWithAssistant} onOptimize={optimizeWithAssistant} />
            <Tooltip label={justSaved ? t("ui.queryPane.saved") : t("ui.queryPane.saveTheQuery")} shortcut={justSaved ? undefined : `${MOD}S`} side="top">
              <Button
                variant="ghost-muted"
                size="icon-sm"
                className={justSaved ? "text-success-foreground hover:text-success-foreground" : undefined}
                onClick={() => saveActiveQuery()}
                disabled={!savedStorageKey}
                aria-label={t("ui.queryPane.saveTheQuery")}
              >
                {justSaved ? <Check /> : <Save />}
              </Button>
            </Tooltip>
            <div className="ml-auto flex items-center gap-0.5">
              {capabilities.transactions && (
                <TransactionButton
                  open={inTransaction}
                  disabled={txBusy || running}
                  onBegin={() => runTransaction("begin")}
                  onCommit={() => runTransaction("commit")}
                  onRollback={() => runTransaction("rollback")}
                />
              )}
              <Tooltip label={safeMode ? t("ui.queryPane.safeModeOn") : t("ui.queryPane.safeModeOff")} side="top">
                <Button
                  variant="ghost-muted"
                  size="icon-sm"
                  className={safeMode ? "text-success-foreground hover:text-success-foreground" : undefined}
                  onClick={() => setSafeMode((v) => !v)}
                  aria-pressed={safeMode}
                  aria-label={t("ui.queryPane.safeMode")}
                >
                  {safeMode ? <ShieldCheck /> : <ShieldOff />}
                </Button>
              </Tooltip>
              <Tooltip label={t("ui.queryPane.moreActions")} side="top">
                <Button ref={moreButtonRef} variant="ghost-muted" size="icon-sm" onClick={() => setMoreOpen((v) => !v)} aria-label={t("ui.queryPane.more")} aria-expanded={moreOpen}>
                  <MoreHorizontal />
                </Button>
              </Tooltip>
              <Popover open={moreOpen} onClose={() => setMoreOpen(false)} anchorRef={moreButtonRef} className="w-52 p-1">
                <div role="menu" className="[&>button]:flex [&>button]:w-full [&>button]:items-center [&>button]:gap-2.5 [&>button]:rounded-md [&>button]:px-2 [&>button]:py-1.5 [&>button]:text-left [&>button]:text-[13px] [&>button]:cursor-pointer [&>button:hover]:bg-accent [&_svg]:size-3.5 [&_svg]:text-muted-foreground">
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setMoreOpen(false);
                      if (!historyOpen && historyStorageKey) setHistory(loadHistory(historyStorageKey));
                      if (!historyOpen && savedStorageKey) setSavedQueries(loadSaved(savedStorageKey));
                      setHistoryOpen((open) => !open);
                    }}
                  >
                    <History /> {historyOpen ? t("ui.queryPane.hideHistory") : t("ui.queryPane.history")}
                  </button>
                  <div className="mx-1 my-1 h-px bg-border" />
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); editorRef.current?.format(); }}>
                    <AlignLeft /> {t("ui.queryPane.formatSql")}
                    <Keys keys={`${SHIFT}${ALT}F`} className="ml-auto" />
                  </button>
                  <div className="mx-1 my-1 h-px bg-border" />
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); importQuery(); }}>
                    <Upload /> {t("ui.queryPane.openSqlFile")}
                  </button>
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); exportQuery(); }}>
                    <Download /> {t("ui.queryPane.saveSqlFile")}
                  </button>
                </div>
              </Popover>
            </div>
          </div>
        </div>

        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label={t("ui.queryPane.resizeEditor")}
          onPointerDown={startResize}
          onDoubleClick={() => {
            setSplit(50);
            try {
              localStorage.removeItem("dbunny:editorSplit");
            } catch {
              // nothing stored
            }
          }}
          className="relative z-10 -my-1 h-2 shrink-0 cursor-row-resize after:absolute after:inset-x-0 after:top-1/2 after:h-0.5 after:-translate-y-1/2 after:bg-primary/60 after:opacity-0 after:transition-opacity hover:after:opacity-100"
        />

        {/* Results panel: tabs appear once there are messages or a plan to show */}
        <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
          {(plan || (result?.notices?.length ?? 0) > 0) && (
            <div className="flex shrink-0 items-center border-b border-border px-2 py-1.5">
              <SegmentedTabs
                value={view}
                onChange={showView}
                options={[
                  { value: "results", label: t("ui.queryPane.results") },
                  {
                    value: "messages",
                    label: (
                      <>
                        {t("ui.queryPane.messages")} <span className="text-xs tabular-nums text-muted-foreground">{result?.notices?.length ?? 0}</span>
                      </>
                    ),
                  },
                  ...(plan ? [{ value: "plan" as const, label: t("ui.queryPane.plan") }] : []),
                ]}
              />
            </div>
          )}
          <div className="flex-1 min-h-0 overflow-hidden">
            {view === "plan" && plan && !error ? (
              <PlanView explain={plan} />
            ) : view === "messages" && !error ? (
              <div className="h-full overflow-auto">
                <NoticeList notices={result?.notices ?? []} />
              </div>
            ) : (
              <QueryResults result={result} error={error} running={running} onFix={assistant.available && assistant.enabled ? fixWithAssistant : undefined} />
            )}
          </div>
          <ResultsBar
            result={result}
            error={error}
            runningSince={runningSince}
            onCancel={cancelQuery}
            exportName={title.replace(/\s+/g, "_").toLowerCase()}
          />
        </div>
      </div>
      {renderSidePanel(
        <SideCard open={historyOpen} widthClass="w-80" label={t("ui.queryPane.queryLibrary")}>
          <HistoryPanel
            view={libraryView}
            onViewChange={setLibraryView}
            entries={history}
            saved={savedQueries}
            onOpen={(sql) => openFromHistory(sql)}
            onOpenSaved={(query) => openFromHistory(query.sql, { label: query.name, savedId: query.id })}
            onRemoveSaved={(id) => {
              if (!savedStorageKey) return;
              setSavedQueries(removeSaved(savedStorageKey, id));
              if (linkedSavedId === id) setLinkedSavedId(undefined);
            }}
            onRemove={(id) => historyStorageKey && setHistory(removeFromHistory(historyStorageKey, id))}
            onClear={() => {
              if (historyStorageKey) clearHistory(historyStorageKey);
              setHistory([]);
            }}
            onClose={() => setHistoryOpen(false)}
          />
        </SideCard>,
      )}
      <Dialog
        open={saveName !== null}
        onClose={() => setSaveName(null)}
        icon={<Save />}
        title={t("ui.queryPane.saveQuery")}
        description={t("ui.queryPane.saveHint")}
        footer={
          <>
            <Button variant="neutral" size="sm" onClick={() => setSaveName(null)}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" size="sm" disabled={!saveName?.trim()} onClick={() => saveActiveQuery(saveName ?? "")}>
              {t("common.save")}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (saveName?.trim()) saveActiveQuery(saveName);
          }}
        >
          <Input value={saveName ?? ""} onChange={(e) => setSaveName(e.target.value)} placeholder={t("ui.queryPane.namePlaceholder")} autoFocus aria-label={t("ui.queryPane.name")} />
        </form>
      </Dialog>
    </div>
  );
}
