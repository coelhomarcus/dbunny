import { useState, useCallback, useRef, useEffect } from "react";
import { useConnection } from "../contexts/useConnection";
import { useTab } from "../contexts/workspace";
import { tabState } from "../lib/tabState";
import type { CompletionSchema, QueryResult } from "@/types";
import QueryResults from "../components/QueryResults";
import SqlEditor, { type SqlEditorHandle } from "../components/SqlEditor";
import { Plus, X, Upload, Download, Loader, ShieldCheck, ShieldOff, AlertTriangle } from "lucide-react";
import ExportMenu from "../components/ExportMenu";
import Button from "../components/ui/Button";

type TabMeta = {
  id: number;
  label: string;
};

type TabPayload = {
  sql: string;
  result: QueryResult | null;
  error: string | null;
};

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
  const { tabId } = useTab();
  const { api, sessionId } = useConnection();
  const saved = tabState(tabId, "queryEditor", createQueryEditorState);
  repairQueryEditorState(saved);
  const [tabMetas, setTabMetas] = useState<TabMeta[]>(saved.tabMetas);
  const [activeId, setActiveId] = useState(saved.activeId);
  const initial = saved.payloads.get(saved.activeId)!;
  const [result, setResult] = useState<QueryResult | null>(initial.result);
  const [error, setError] = useState<string | null>(initial.error);
  const [running, setRunning] = useState(false);
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
  }

  function addTab() {
    saveCurrentSql();
    const id = saved.nextId++;
    const label = `Query ${id}`;
    saved.payloads.set(id, { sql: "", result: null, error: null });
    const newMetas = [...tabMetas, { id, label }];
    setTabMetas(newMetas);
    saved.tabMetas = newMetas;
    setActiveId(id);
    saved.activeId = id;
    editorRef.current?.setValue("");
    setResult(null);
    setError(null);
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

  const executeQuery = useCallback(async (sql: string) => {
    setRunning(true);
    setError(null);
    setResult(null);
    const delay = new Promise<void>((r) => setTimeout(r, 400));
    try {
      const [res] = await Promise.all([api.query(sql), delay]);
      setResult(res);
      const payload = saved.payloads.get(activeIdRef.current);
      if (payload) { payload.result = res; payload.error = null; }
      if (DDL_PATTERN.test(sql)) loadCompletionSchema();
    } catch (err) {
      await delay;
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : "Query failed";
      setError(msg);
      const payload = saved.payloads.get(activeIdRef.current);
      if (payload) { payload.result = null; payload.error = msg; }
    } finally {
      setRunning(false);
    }
  }, [loadCompletionSchema, api, saved]);

  const runQuery = useCallback(async () => {
    const editor = editorRef.current;
    if (!editor) return;
    const sql = editor.getSelectionOrValue();
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
  }, [safeMode, executeQuery]);

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
    <div className="h-full flex flex-col">
      {pendingDangers && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-[2px]">
          <div className="bg-card border border-border rounded-2xl shadow-2xl w-full max-w-sm mx-4 overflow-hidden">
            <div className="px-5 pt-5 pb-4">
              <div className="flex items-center gap-2.5 mb-1">
                <div className="flex items-center justify-center w-7 h-7 rounded-full bg-destructive/15">
                  <AlertTriangle size={14} className="text-destructive-foreground" />
                </div>
                <h3 className="text-sm font-semibold text-foreground">Query blocked by Safe Mode</h3>
              </div>
              <p className="text-sm text-muted-foreground mt-2">
                {pendingDangers.length === 1
                  ? "A potentially destructive operation was detected."
                  : `${pendingDangers.length} potentially destructive operations were detected.`}
              </p>
            </div>

            <div className="mx-5 mb-4 rounded-xl border border-border bg-muted divide-y divide-border">
              {pendingDangers.map((d, i) => (
                <div key={i} className="px-3 py-2.5">
                  <span className="font-mono text-xs font-medium text-destructive-foreground block">{d.rule}</span>
                  <span className="text-xs text-muted-foreground block mt-0.5">{d.detail}</span>
                </div>
              ))}
            </div>

            <div className="flex items-center gap-2 px-5 py-3.5 border-t border-border bg-muted">
              <Button variant="destructive" size="sm" onClick={confirmDangerousQuery}>
                Run anyway
              </Button>
              <Button variant="ghost-muted" size="sm" onClick={cancelDangerousQuery}>
                Cancel
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Editor panel */}
      <div className="flex flex-col flex-1 min-h-0 border-b border-border">
        {/* Query tabs as compact pills: a lighter level than the connection tabs above */}
        <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 select-none">
          <div role="tablist" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
            {tabMetas.map((tab) => {
              const active = activeId === tab.id;
              return (
                <button
                  key={tab.id}
                  role="tab"
                  aria-selected={active}
                  onClick={() => switchTab(tab.id)}
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
              );
            })}
            <Button variant="ghost-muted" size="icon-xs" className="ml-0.5 shrink-0" onClick={addTab} title="New query tab">
              <Plus />
            </Button>
          </div>
        </div>

        {/* SQL editor */}
        <div className="flex-1 min-h-0">
          <SqlEditor
            onMount={(handle) => { editorRef.current = handle; }}
            initialValue={saved.payloads.get(activeId)?.sql ?? ""}
            schema={completionSchema}
            onRun={runQuery}
          />
        </div>

        {/* Toolbar */}
        <div className="flex items-center gap-3 px-3 py-2 bg-muted border-t border-border select-none">
          <Button variant="primary" size="sm" className="px-4" onClick={runQuery} disabled={running}>
            <span className={`transition-opacity duration-200 ${running ? "opacity-0" : "opacity-100"}`}>
              Run
            </span>
            <Loader className={`size-4 animate-spin absolute inset-0 m-auto transition-opacity duration-200 ${running ? "opacity-100" : "opacity-0"}`} />
          </Button>
          {result && (
            <span className="text-sm text-muted-foreground">
              {result.rowsAffected != null
                ? `${result.rowsAffected} rows affected in ${result.duration}ms`
                : `${result.rowCount} rows in ${result.duration}ms`}
            </span>
          )}
          <div className="flex items-center gap-1 ml-auto">
            {result && result.columns.length > 0 && (
              <ExportMenu
                columns={result.columns}
                rows={result.rows}
                defaultName={tabMetas.find((t) => t.id === activeId)?.label.replace(/\s+/g, "_").toLowerCase() ?? "query"}
                side="up"
              />
            )}
            <Button
              variant={safeMode ? "ghost" : "ghost-muted"}
              size="sm"
              className={safeMode ? "text-success-foreground" : undefined}
              onClick={() => setSafeMode((v) => !v)}
              title={safeMode ? "Safe Mode ON - click to disable" : "Safe Mode OFF - click to enable"}
            >
              {safeMode ? <ShieldCheck /> : <ShieldOff />}
              {safeMode ? "Safe" : "Unsafe"}
            </Button>
            <Button variant="ghost-muted" size="sm" onClick={importQuery} title="Import .sql file">
              <Upload />
              Import
            </Button>
            <Button variant="ghost-muted" size="sm" onClick={exportQuery} title="Export as .sql file">
              <Download />
              Export SQL
            </Button>
          </div>
        </div>
      </div>

      {/* Results panel */}
      <div className="flex-1 min-h-0 overflow-hidden">
        <QueryResults result={result} error={error} running={running} />
      </div>
    </div>
  );
}
