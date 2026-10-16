import { useState, useCallback, useRef, useEffect } from "react";
import { useConnection } from "../contexts/useConnection";
import { useTab } from "../contexts/workspace";
import { tabState } from "../lib/tabState";
import type { CompletionSchema, ExplainResult, QueryResult } from "@/types";
import QueryResults from "../components/QueryResults";
import SqlEditor, { type SqlEditorHandle } from "../components/SqlEditor";
import { Plus, X, Upload, Download, ShieldCheck, ShieldOff, AlertTriangle, History, Check, MoreHorizontal, AlignLeft, Save } from "lucide-react";
import RunButton from "../components/query/RunButton";
import TransactionButton from "../components/query/TransactionButton";
import ResultsBar from "../components/query/ResultsBar";
import Tooltip from "../components/Tooltip";
import { createPortal } from "react-dom";
import { useRightPanel } from "../contexts/rightPanel";
import type { RunScope } from "../components/SqlEditor";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import Popover from "../components/ui/Popover";
import PlanView from "../components/query/PlanView";
import NoticeList from "../components/query/NoticeList";
import HistoryPanel, { type LibraryView } from "../components/query/HistoryPanel";
import Dialog from "../components/ui/Dialog";
import Input from "../components/ui/Input";
import { loadSaved, removeSaved, saveQuery, savedKey, type SavedQuery } from "../lib/savedQueries";
import { addToHistory, clearHistory, errorSummary, historyKey, loadHistory, removeFromHistory, type HistoryEntry } from "../lib/queryHistory";
import Button from "../components/ui/Button";
import ConfirmDialog from "../components/ui/ConfirmDialog";
import { isMac } from "../lib/platform";

const MOD = isMac ? "⌘" : "Ctrl+";
const SHIFT = isMac ? "⇧" : "Shift+";
const ALT = isMac ? "⌥" : "Alt+";

type TabMeta = {
  id: number;
  label: string;
  /** The saved query this tab was opened from or saved as; saving again updates it. */
  savedId?: string;
};

type ResultView = "results" | "messages" | "plan";

type TabPayload = {
  sql: string;
  result: QueryResult | null;
  error: string | null;
  plan?: ExplainResult | null;
  view?: ResultView;
};

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

// Kept per window tab, so it survives navigating to a table and back
type QueryEditorState = {
  nextId: number;
  tabMetas: TabMeta[];
  activeId: number;
  safeMode: boolean;
  payloads: Map<number, TabPayload>;
  // Autocomplete catalog, reused when coming back to the page
  completion: { sessionId: string | null; schema: CompletionSchema } | null;
};

function createQueryEditorState(): QueryEditorState {
  return {
    nextId: 2,
    tabMetas: [{ id: 1, label: "Query 1" }],
    activeId: 1,
    safeMode: true,
    payloads: new Map([[1, { sql: "SELECT 1;", result: null, error: null }]]),
    completion: null,
  };
}

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
      dangers.push({ rule: "DROP DATABASE", detail: "Deletes the entire database permanently." });
    } else if (/\bDROP\s+SCHEMA\b/.test(stmt)) {
      dangers.push({ rule: "DROP SCHEMA", detail: "Deletes the schema and all its objects permanently." });
    } else if (/\bDROP\s+TABLE\b/.test(stmt)) {
      dangers.push({ rule: "DROP TABLE", detail: "Deletes the table and all its data permanently." });
    } else if (/\bTRUNCATE\b/.test(stmt)) {
      dangers.push({ rule: "TRUNCATE", detail: "Deletes all rows from the table instantly." });
    } else if (/\bDELETE\b/.test(stmt) && !/\bWHERE\b/.test(stmt)) {
      dangers.push({ rule: "DELETE without WHERE", detail: "Deletes every row in the table, no filter applied." });
    } else if (/\bUPDATE\b/.test(stmt) && /\bSET\b/.test(stmt) && !/\bWHERE\b/.test(stmt)) {
      dangers.push({ rule: "UPDATE without WHERE", detail: "Updates every row in the table, no filter applied." });
    } else if (/\bALTER\s+TABLE\b/.test(stmt) && /\bDROP\s+COLUMN\b/.test(stmt)) {
      dangers.push({ rule: "ALTER TABLE DROP COLUMN", detail: "Removes the column and its data permanently." });
    }
  }

  return dangers;
}

/**
 * Makes the saved state consistent before it's used: only tabs that still have
 * their payload, and an active tab that exists. Protects against state left
 * inconsistent by older versions instead of crashing on mount.
 */
function repairQueryEditorState(saved: QueryEditorState) {
  saved.tabMetas = saved.tabMetas.filter((t) => saved.payloads.has(t.id));
  if (saved.tabMetas.length === 0) {
    const fresh = createQueryEditorState();
    saved.tabMetas = fresh.tabMetas;
    saved.payloads = fresh.payloads;
    saved.nextId = Math.max(saved.nextId, fresh.nextId);
  }
  if (!saved.tabMetas.some((t) => t.id === saved.activeId)) {
    saved.activeId = saved.tabMetas[0].id;
  }
}

export default function QueryEditor() {
  const { tabId, isActive } = useTab();
  const rightPanel = useRightPanel();
  const renderSidePanel = (panel: React.ReactNode) => (rightPanel ? createPortal(panel, rightPanel) : null);
  const { api, sessionId, connectionInfo } = useConnection();
  const historyStorageKey = connectionInfo ? historyKey(connectionInfo) : null;
  const [history, setHistory] = useState<HistoryEntry[]>(() => (historyStorageKey ? loadHistory(historyStorageKey) : []));
  const [historyOpen, setHistoryOpen] = useState(false);
  const [libraryView, setLibraryView] = useState<LibraryView>("history");
  const savedStorageKey = connectionInfo ? savedKey(connectionInfo) : null;
  const [savedQueries, setSavedQueries] = useState<SavedQuery[]>(() => (savedStorageKey ? loadSaved(savedStorageKey) : []));
  // Name being typed when saving a query for the first time
  const [saveName, setSaveName] = useState<string | null>(null);
  const [justSaved, setJustSaved] = useState(false);
  // Why the last format failed, shown for a few seconds next to Run
  const [formatError, setFormatError] = useState<string | null>(null);
  const formatErrorTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const showFormatError = useCallback((message: string) => {
    setFormatError(message);
    clearTimeout(formatErrorTimer.current);
    formatErrorTimer.current = setTimeout(() => setFormatError(null), 5000);
  }, []);
  // Tabs of saved queries whose text differs from what's saved
  const [dirtyIds, setDirtyIds] = useState<Set<number>>(() => new Set());
  const [renamingId, setRenamingId] = useState<number | null>(null);
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
  const saved = tabState(tabId, "queryEditor", createQueryEditorState);
  repairQueryEditorState(saved);
  const [tabMetas, setTabMetas] = useState<TabMeta[]>(saved.tabMetas);
  const [activeId, setActiveId] = useState(saved.activeId);
  const initial = saved.payloads.get(saved.activeId)!;
  const [result, setResult] = useState<QueryResult | null>(initial.result);
  const [error, setError] = useState<string | null>(initial.error);
  const [running, setRunning] = useState(false);
  const [plan, setPlan] = useState<ExplainResult | null>(initial.plan ?? null);
  const [view, setView] = useState<ResultView>(initial.view ?? "results");
  const [inTransaction, setInTransaction] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  // When the running query started, for the elapsed time under the results
  const [runningSince, setRunningSince] = useState<number | null>(null);
  const [moreOpen, setMoreOpen] = useState(false);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const [safeMode, setSafeMode] = useState(saved.safeMode);
  const [pendingDangers, setPendingDangers] = useState<DangerInfo[] | null>(null);
  const pendingSqlRef = useRef<string | null>(null);
  const [completionSchema, setCompletionSchema] = useState<CompletionSchema | null>(() =>
    saved.completion?.sessionId === sessionId ? saved.completion.schema : null,
  );

  const editorRef = useRef<SqlEditorHandle | null>(null);
  const activeIdRef = useRef(activeId);

  useEffect(() => {
    activeIdRef.current = activeId;
  }, [activeId]);

  useEffect(() => {
    saved.safeMode = safeMode;
  }, [safeMode, saved]);

  const loadCompletionSchema = useCallback(() => {
    api
      .getCompletionSchema()
      .then((schema) => {
        saved.completion = { sessionId, schema };
        setCompletionSchema(schema);
      })
      .catch(() => {});
  }, [api, sessionId, saved]);

  useEffect(() => {
    loadCompletionSchema();
  }, [loadCompletionSchema]);

  // The transaction lives on the server side of the connection; ask whether one is open
  useEffect(() => {
    api.transactionOpen().then(setInTransaction).catch(() => {});
  }, [api]);

  // On unmount (e.g. opening a table), keep the SQL being edited. The tab list and
  // active tab are already written to `saved` by switchTab/addTab/closeTab; writing
  // them here too would restore stale values (a closed tab coming back as active).
  useEffect(() => {
    return () => {
      const editor = editorRef.current;
      const payload = saved.payloads.get(activeIdRef.current);
      if (editor && payload) payload.sql = editor.getValue();
    };
  }, [saved]);

  function saveCurrentSql() {
    const editor = editorRef.current;
    if (!editor) return;
    const payload = saved.payloads.get(activeId);
    if (payload) payload.sql = editor.getValue();
  }

  function switchTab(id: number) {
    saveCurrentSql();
    setActiveId(id);
    saved.activeId = id;
    const payload = saved.payloads.get(id)!;
    editorRef.current?.setValue(payload.sql);
    setResult(payload.result);
    setError(payload.error);
    setPlan(payload.plan ?? null);
    setView(payload.view ?? "results");
  }

  function addTab(options: { label?: string; savedId?: string } = {}) {
    saveCurrentSql();
    const id = saved.nextId++;
    const label = options.label ?? `Query ${id}`;
    saved.payloads.set(id, { sql: "", result: null, error: null });
    const newMetas = [...tabMetas, { id, label, savedId: options.savedId }];
    setTabMetas(newMetas);
    saved.tabMetas = newMetas;
    setActiveId(id);
    saved.activeId = id;
    editorRef.current?.setValue("");
    setResult(null);
    setError(null);
    setPlan(null);
    setView("results");
  }

  // Read from the editor's change handler, which is created once
  const latest = useRef({ tabMetas, activeId, savedQueries });
  useEffect(() => {
    latest.current = { tabMetas, activeId, savedQueries };
  });

  /** Marks a tab dirty when its text no longer matches its saved query. */
  function setDirty(id: number, dirty: boolean) {
    setDirtyIds((prev) => {
      if (prev.has(id) === dirty) return prev;
      const next = new Set(prev);
      if (dirty) next.add(id);
      else next.delete(id);
      return next;
    });
  }

  const handleEditorChange = useCallback((sql: string) => {
    const { tabMetas: metas, activeId: id, savedQueries: list } = latest.current;
    const savedId = metas.find((t) => t.id === id)?.savedId;
    if (!savedId) return;
    const stored = list.find((q) => q.id === savedId);
    setDirty(id, stored !== undefined && stored.sql !== sql);
  }, []);

  /** Renames a tab; a saved query's tab renames the query too. */
  function renameTab(id: number, label: string) {
    setRenamingId(null);
    const name = label.trim();
    const meta = tabMetas.find((t) => t.id === id);
    if (!meta || !name || name === meta.label) return;
    updateTabMeta(id, { label: name });
    const stored = meta.savedId ? savedQueries.find((q) => q.id === meta.savedId) : undefined;
    if (stored && savedStorageKey) setSavedQueries(saveQuery(savedStorageKey, { id: stored.id, name, sql: stored.sql }).list);
  }

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

  /** Opens SQL from the history in a new query tab, leaving the current one as it is. */
  function openFromHistory(sql: string, options: { label?: string; savedId?: string } = {}) {
    // A saved query that's already open is shown instead of opened twice
    const open = options.savedId && tabMetas.find((t) => t.savedId === options.savedId);
    if (open) return switchTab(open.id);
    addTab(options);
    const payload = saved.payloads.get(saved.activeId);
    if (payload) payload.sql = sql;
    editorRef.current?.setValue(sql);
  }

  function updateTabMeta(id: number, patch: Partial<TabMeta>) {
    const newMetas = tabMetas.map((t) => (t.id === id ? { ...t, ...patch } : t));
    setTabMetas(newMetas);
    saved.tabMetas = newMetas;
  }

  /** Saves the active tab's SQL: updates its saved query, or asks for a name the first time. */
  function saveActiveQuery(name?: string) {
    if (!savedStorageKey) return;
    const meta = tabMetas.find((t) => t.id === activeId);
    const sql = editorRef.current?.getValue() ?? "";
    const existing = meta?.savedId ? savedQueries.find((q) => q.id === meta.savedId) : undefined;
    if (!existing && name === undefined) {
      setSaveName(/^Query \d+$/.test(meta?.label ?? "") ? "" : (meta?.label ?? ""));
      return;
    }
    const { list, saved: stored } = saveQuery(savedStorageKey, { id: existing?.id, name: name ?? existing!.name, sql });
    setSavedQueries(list);
    updateTabMeta(activeId, { label: stored.name, savedId: stored.id });
    setDirty(activeId, false);
    setSaveName(null);
    setJustSaved(true);
    setTimeout(() => setJustSaved(false), 1500);
  }

  function closeTab(e: React.MouseEvent, id: number) {
    e.stopPropagation();
    if (tabMetas.length === 1) return;
    const index = tabMetas.findIndex((t) => t.id === id);
    const newMetas = tabMetas.filter((t) => t.id !== id);
    saved.payloads.delete(id);
    let newActiveId = activeId;
    if (id === activeId) {
      const newIndex = Math.min(index, newMetas.length - 1);
      newActiveId = newMetas[newIndex].id;
    }
    setTabMetas(newMetas);
    saved.tabMetas = newMetas;
    setActiveId(newActiveId);
    saved.activeId = newActiveId;
    if (id === activeId) {
      const payload = saved.payloads.get(newActiveId)!;
      editorRef.current?.setValue(payload.sql);
      setResult(payload.result);
      setError(payload.error);
      setPlan(payload.plan ?? null);
      setView(payload.view ?? "results");
    }
  }

  async function exportQuery() {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    const sql = editorRef.current?.getValue() ?? saved.payloads.get(activeId)?.sql ?? "";
    const meta = tabMetas.find((t) => t.id === activeId);
    const defaultName = `${(meta?.label ?? "query").replace(/\s+/g, "_").toLowerCase()}.sql`;
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
      const payload = saved.payloads.get(activeId);
      if (payload) payload.sql = sql;
    }
  }

  /** Shows a result view and remembers it for the active query tab. */
  const showView = useCallback((next: ResultView) => {
    setView(next);
    const payload = saved.payloads.get(activeIdRef.current);
    if (payload) payload.view = next;
  }, [saved]);

  const runTransaction = useCallback(async (command: "begin" | "commit" | "rollback") => {
    setTxBusy(true);
    setError(null);
    try {
      if (command === "begin") await api.beginTransaction();
      else if (command === "commit") await api.commitTransaction();
      else await api.rollbackTransaction();
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Transaction failed");
      setResult(null);
      showView("results");
    } finally {
      // Commit and rollback close the transaction even when they fail
      setInTransaction(await api.transactionOpen().catch(() => false));
      setTxBusy(false);
    }
  }, [api, showView]);

  const executeQuery = useCallback(async (sql: string) => {
    const txCommand = transactionCommand(sql);
    if (txCommand) {
      await runTransaction(txCommand);
      return;
    }
    setRunning(true);
    setRunningSince(Date.now());
    setError(null);
    setResult(null);
    const delay = new Promise<void>((r) => setTimeout(r, 400));
    try {
      const [res] = await Promise.all([api.query(sql), delay]);
      setResult(res);
      const payload = saved.payloads.get(activeIdRef.current);
      if (payload) { payload.result = res; payload.error = null; }
      if (historyStorageKey) {
        setHistory(addToHistory(historyStorageKey, {
          sql,
          durationMs: res.duration,
          rows: res.rowsAffected ?? res.rowCount,
          error: null,
        }));
      }
      // A script that only printed messages opens on them
      showView(res.columns.length === 0 && (res.notices?.length ?? 0) > 0 ? "messages" : "results");
      if (DDL_PATTERN.test(sql)) loadCompletionSchema();
    } catch (err) {
      await delay;
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : "Query failed";
      setError(msg);
      const payload = saved.payloads.get(activeIdRef.current);
      if (payload) { payload.result = null; payload.error = msg; }
      if (historyStorageKey) {
        setHistory(addToHistory(historyStorageKey, { sql, durationMs: null, rows: null, error: errorSummary(msg) }));
      }
      // A lost transaction connection ends the transaction; reflect it in the toolbar
      if (inTransaction) api.transactionOpen().then(setInTransaction).catch(() => setInTransaction(false));
      showView("results");
    } finally {
      setRunning(false);
      setRunningSince(null);
    }
  }, [loadCompletionSchema, api, saved, runTransaction, showView, historyStorageKey, inTransaction]);

  const explain = useCallback(async (analyze: boolean) => {
    // EXPLAIN takes one statement: the selection, or the one at the cursor
    const sql = editorRef.current?.getCurrentStatement() ?? "";
    if (!sql.trim()) return;
    setRunning(true);
    setRunningSince(Date.now());
    setError(null);
    try {
      const res = await api.explain(sql, analyze);
      setPlan(res);
      const payload = saved.payloads.get(activeIdRef.current);
      if (payload) payload.plan = res;
      showView("plan");
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "EXPLAIN failed");
      showView("results");
    } finally {
      setRunning(false);
      setRunningSince(null);
    }
  }, [api, saved, showView]);

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
    if (!isActive) return;
    function onKeyDown(e: KeyboardEvent) {
      const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!mod || e.shiftKey || e.altKey || e.key.toLowerCase() !== "s") return;
      e.preventDefault();
      if (document.querySelector("[role=dialog][aria-modal]")) return;
      saveRef.current();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isActive]);

  // Esc stops the running query (only in the visible tab)
  useEffect(() => {
    if (!running || !isActive) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && !document.querySelector("[role=dialog][aria-modal]")) cancelQuery();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [running, isActive, cancelQuery]);

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
          title="Query blocked by Safe Mode"
          description={
            pendingDangers?.length === 1
              ? "A potentially destructive operation was detected."
              : `${pendingDangers?.length ?? 0} potentially destructive operations were detected.`
          }
          cancelLabel="Cancel"
          confirmLabel="Run anyway"
        >
          <div className="rounded-xl border border-border bg-muted divide-y divide-border">
            {(pendingDangers ?? []).map((d, i) => (
              <div key={i} className="px-3 py-2.5">
                <span className="font-mono text-xs font-medium text-destructive-foreground block">{d.rule}</span>
                <span className="text-xs text-muted-foreground block mt-0.5">{d.detail}</span>
              </div>
            ))}
          </div>
        </ConfirmDialog>

        {/* Editor panel: its height is dragged with the handle under it */}
        <div className="flex min-h-0 shrink-0 grow-0 flex-col border-b border-border" style={{ flexBasis: `${split}%` }}>
          {/* Query tabs as compact pills: a lighter level than the connection tabs above */}
          <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 select-none">
            <div role="tablist" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
              {tabMetas.map((tab) => {
                const active = activeId === tab.id;
                return (
                  renamingId === tab.id ? (
                    <input
                      key={tab.id}
                      autoFocus
                      defaultValue={tab.label}
                      aria-label="Tab name"
                      onFocus={(e) => e.currentTarget.select()}
                      onBlur={(e) => renameTab(tab.id, e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") renameTab(tab.id, e.currentTarget.value);
                        if (e.key === "Escape") setRenamingId(null);
                      }}
                      className="h-7 w-36 shrink-0 rounded-md border border-ring bg-card px-2 text-[13px] outline-none ring-3 ring-ring/20"
                    />
                  ) : (
                  <button
                    key={tab.id}
                    role="tab"
                    aria-selected={active}
                    onClick={() => switchTab(tab.id)}
                    onDoubleClick={() => setRenamingId(tab.id)}
                    // Transition only while hovered, so switching tabs swaps styles instantly
                    className={`group flex h-7 shrink-0 items-center gap-1 rounded-md pl-2.5 text-[13px] whitespace-nowrap cursor-pointer ${
                      tabMetas.length > 1 ? "pr-1" : "pr-2.5"
                    } ${
                      active
                        ? "bg-accent font-medium text-foreground"
                        : "text-muted-foreground hover:bg-muted hover:text-foreground hover:transition-colors"
                    }`}
                  >
                    {tab.label}
                    {dirtyIds.has(tab.id) && <span className="size-1.5 shrink-0 rounded-full bg-primary" aria-label="Unsaved changes" />}
                    {tabMetas.length > 1 && (
                      <span
                        onClick={(e) => closeTab(e, tab.id)}
                        className={`flex size-4 items-center justify-center rounded transition-colors cursor-pointer hover:bg-foreground/10 ${
                          active
                            ? "text-muted-foreground hover:text-foreground"
                            : "text-transparent group-hover:text-muted-foreground hover:text-foreground!"
                        }`}
                      >
                        <X size={10} />
                      </span>
                    )}
                  </button>
                  )
                );
              })}
              <Button variant="ghost-muted" size="icon-xs" className="ml-0.5 shrink-0" onClick={() => addTab()} aria-label="New query tab">
                <Plus />
              </Button>
            </div>
            <Button
              variant="ghost-muted"
              size="sm"
              className="shrink-0"
              data-active={historyOpen}
              aria-pressed={historyOpen}
              onClick={() => {
                // Other tabs on the same database may have changed them since this one loaded
                if (!historyOpen && historyStorageKey) setHistory(loadHistory(historyStorageKey));
                if (!historyOpen && savedStorageKey) setSavedQueries(loadSaved(savedStorageKey));
                setHistoryOpen((open) => !open);
              }}
            >
              <History />
              History
            </Button>
          </div>

          {/* SQL editor */}
          <div className="flex-1 min-h-0">
            <SqlEditor
              onMount={(handle) => { editorRef.current = handle; }}
              initialValue={saved.payloads.get(activeId)?.sql ?? ""}
              schema={completionSchema}
              onRun={runQuery}
              onChange={handleEditorChange}
              onFormatError={showFormatError}
            />
          </div>

          {/* Actions: run on the left, the rest quieter on the right */}
          <div className="flex h-11 shrink-0 items-center gap-2 border-t border-border px-2.5 select-none">
            <RunButton disabled={running} onRun={runQuery} onExplain={explain} />
            {formatError && <span className="min-w-0 truncate text-xs text-destructive-foreground">Couldn't format: {formatError}</span>}
            <div className="ml-auto flex items-center gap-0.5">
              <TransactionButton
                open={inTransaction}
                disabled={txBusy || running}
                onBegin={() => runTransaction("begin")}
                onCommit={() => runTransaction("commit")}
                onRollback={() => runTransaction("rollback")}
              />
              <Tooltip label={safeMode ? "Safe mode: asks before destructive statements" : "Safe mode off: destructive statements run directly"} side="top">
                <Button
                  variant="ghost-muted"
                  size="icon-sm"
                  className={safeMode ? "text-success-foreground hover:text-success-foreground" : undefined}
                  onClick={() => setSafeMode((v) => !v)}
                  aria-pressed={safeMode}
                  aria-label="Safe mode"
                >
                  {safeMode ? <ShieldCheck /> : <ShieldOff />}
                </Button>
              </Tooltip>
              <Tooltip label={justSaved ? "Saved" : `Save the query (${MOD}S)`} side="top">
                <Button
                  variant="ghost-muted"
                  size="icon-sm"
                  className={justSaved ? "text-success-foreground hover:text-success-foreground" : undefined}
                  onClick={() => saveActiveQuery()}
                  disabled={!savedStorageKey}
                  aria-label="Save the query"
                >
                  {justSaved ? <Check /> : <Save />}
                </Button>
              </Tooltip>
              <Tooltip label="More actions" side="top">
                <Button ref={moreButtonRef} variant="ghost-muted" size="icon-sm" onClick={() => setMoreOpen((v) => !v)} aria-label="More" aria-expanded={moreOpen}>
                  <MoreHorizontal />
                </Button>
              </Tooltip>
              <Popover open={moreOpen} onClose={() => setMoreOpen(false)} anchorRef={moreButtonRef} className="w-52 p-1">
                <div role="menu" className="[&>button]:flex [&>button]:w-full [&>button]:items-center [&>button]:gap-2.5 [&>button]:rounded-md [&>button]:px-2 [&>button]:py-1.5 [&>button]:text-left [&>button]:text-[13px] [&>button]:cursor-pointer [&>button:hover]:bg-accent [&_svg]:size-3.5 [&_svg]:text-muted-foreground">
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); editorRef.current?.format(); }}>
                    <AlignLeft /> Format SQL
                    <kbd className="ml-auto font-sans text-[11px] text-muted-foreground">{SHIFT}{ALT}F</kbd>
                  </button>
                  <div className="mx-1 my-1 h-px bg-border" />
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); importQuery(); }}>
                    <Upload /> Open .sql file…
                  </button>
                  <button type="button" role="menuitem" onClick={() => { setMoreOpen(false); exportQuery(); }}>
                    <Download /> Save as .sql file…
                  </button>
                </div>
              </Popover>
            </div>
          </div>
        </div>

        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize editor"
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
                  { value: "results", label: "Results" },
                  {
                    value: "messages",
                    label: (
                      <>
                        Messages <span className="text-xs tabular-nums text-muted-foreground">{result?.notices?.length ?? 0}</span>
                      </>
                    ),
                  },
                  ...(plan ? [{ value: "plan" as const, label: "Plan" }] : []),
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
              <QueryResults result={result} error={error} running={running} />
            )}
          </div>
          <ResultsBar
            result={result}
            error={error}
            runningSince={runningSince}
            onCancel={cancelQuery}
            exportName={tabMetas.find((t) => t.id === activeId)?.label.replace(/\s+/g, "_").toLowerCase() ?? "query"}
          />
        </div>
      </div>
      {/* In the layout's side card, so it's a panel of its own */}
      {historyOpen && renderSidePanel(
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
            // Tabs keep their SQL, just no longer linked to the deleted query
            const newMetas = tabMetas.map((t) => (t.savedId === id ? { ...t, savedId: undefined } : t));
            setTabMetas(newMetas);
            saved.tabMetas = newMetas;
          }}
          onRemove={(id) => historyStorageKey && setHistory(removeFromHistory(historyStorageKey, id))}
          onClear={() => {
            if (historyStorageKey) clearHistory(historyStorageKey);
            setHistory([]);
          }}
          onClose={() => setHistoryOpen(false)}
        />
      )}
      <Dialog
        open={saveName !== null}
        onClose={() => setSaveName(null)}
        icon={<Save />}
        title="Save query"
        description="Kept for this database, in Saved next to the history."
        footer={
          <>
            <Button variant="neutral" size="sm" onClick={() => setSaveName(null)}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" disabled={!saveName?.trim()} onClick={() => saveActiveQuery(saveName ?? "")}>
              Save
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
          <Input value={saveName ?? ""} onChange={(e) => setSaveName(e.target.value)} placeholder="e.g. Orders this month" autoFocus aria-label="Name" />
        </form>
      </Dialog>
    </div>
  );
}
