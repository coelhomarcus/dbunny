import { useState, useCallback, useRef, useEffect } from "react";
import { api, getSessionId } from "../lib/api";
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

// Module-level - survives navigation (component unmount/remount)
let nextId = 2;
let savedTabMetas: TabMeta[] = [{ id: 1, label: "Query 1" }];
let savedActiveId = 1;
let savedSafeMode = true;
const savedPayloads: Map<number, TabPayload> = new Map([
  [1, { sql: "SELECT 1;", result: null, error: null }],
]);
// Autocomplete catalog of the current connection, reused when coming back to the page
let savedCompletion: { sessionId: string | null; schema: CompletionSchema } | null = null;

function cachedCompletionSchema() {
  return savedCompletion?.sessionId === getSessionId() ? savedCompletion.schema : null;
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

export default function QueryEditor() {
  const [tabMetas, setTabMetas] = useState<TabMeta[]>(savedTabMetas);
  const [activeId, setActiveId] = useState(savedActiveId);
  const initial = savedPayloads.get(savedActiveId)!;
  const [result, setResult] = useState<QueryResult | null>(initial.result);
  const [error, setError] = useState<string | null>(initial.error);
  const [running, setRunning] = useState(false);
  const [safeMode, setSafeMode] = useState(savedSafeMode);
  const [pendingDangers, setPendingDangers] = useState<DangerInfo[] | null>(null);
  const pendingSqlRef = useRef<string | null>(null);
  const [completionSchema, setCompletionSchema] = useState<CompletionSchema | null>(cachedCompletionSchema);

  const editorRef = useRef<SqlEditorHandle | null>(null);
  const activeIdRef = useRef(activeId);

  useEffect(() => {
    activeIdRef.current = activeId;
  }, [activeId]);

  useEffect(() => {
    savedSafeMode = safeMode;
  }, [safeMode]);

  const loadCompletionSchema = useCallback(() => {
    const sessionId = getSessionId();
    api
      .getCompletionSchema()
      .then((schema) => {
        savedCompletion = { sessionId, schema };
        setCompletionSchema(schema);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    loadCompletionSchema();
  }, [loadCompletionSchema]);

  useEffect(() => {
    return () => {
      const editor = editorRef.current;
      if (editor) {
        const payload = savedPayloads.get(activeIdRef.current);
        if (payload) payload.sql = editor.getValue();
      }
      savedActiveId = activeIdRef.current;
      savedTabMetas = tabMetas;
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabMetas]);

  function saveCurrentSql() {
    const editor = editorRef.current;
    if (!editor) return;
    const payload = savedPayloads.get(activeId);
    if (payload) payload.sql = editor.getValue();
  }

  function switchTab(id: number) {
    saveCurrentSql();
    setActiveId(id);
    savedActiveId = id;
    const payload = savedPayloads.get(id)!;
    editorRef.current?.setValue(payload.sql);
    setResult(payload.result);
    setError(payload.error);
  }

  function addTab() {
    saveCurrentSql();
    const id = nextId++;
    const label = `Query ${id}`;
    savedPayloads.set(id, { sql: "", result: null, error: null });
    const newMetas = [...tabMetas, { id, label }];
    setTabMetas(newMetas);
    savedTabMetas = newMetas;
    setActiveId(id);
    savedActiveId = id;
    editorRef.current?.setValue("");
    setResult(null);
    setError(null);
  }

  function closeTab(e: React.MouseEvent, id: number) {
    e.stopPropagation();
    if (tabMetas.length === 1) return;
    const index = tabMetas.findIndex((t) => t.id === id);
    const newMetas = tabMetas.filter((t) => t.id !== id);
    savedPayloads.delete(id);
    let newActiveId = activeId;
    if (id === activeId) {
      const newIndex = Math.min(index, newMetas.length - 1);
      newActiveId = newMetas[newIndex].id;
    }
    setTabMetas(newMetas);
    savedTabMetas = newMetas;
    setActiveId(newActiveId);
    savedActiveId = newActiveId;
    if (id === activeId) {
      const payload = savedPayloads.get(newActiveId)!;
      editorRef.current?.setValue(payload.sql);
      setResult(payload.result);
      setError(payload.error);
    }
  }

  async function exportQuery() {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    const sql = editorRef.current?.getValue() ?? savedPayloads.get(activeId)?.sql ?? "";
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
      const payload = savedPayloads.get(activeId);
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
      const payload = savedPayloads.get(activeIdRef.current);
      if (payload) { payload.result = res; payload.error = null; }
      if (DDL_PATTERN.test(sql)) loadCompletionSchema();
    } catch (err) {
      await delay;
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : "Query failed";
      setError(msg);
      const payload = savedPayloads.get(activeIdRef.current);
      if (payload) { payload.result = null; payload.error = msg; }
    } finally {
      setRunning(false);
    }
  }, [loadCompletionSchema]);

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
    <div className="h-full flex flex-col gap-2">
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
      <div className="flex flex-col flex-1 min-h-0 bg-card border border-border rounded-xl overflow-hidden">
        {/* Tabs: the active one joins the editor below (see .joined-tab in index.css) */}
        <div className="tab-strip flex items-end shrink-0 select-none">
          <div className="flex items-end gap-1 flex-1 px-2 pt-1.5 overflow-x-auto overflow-y-hidden">
            {tabMetas.map((tab) => {
              const active = activeId === tab.id;
              return (
                <button
                  key={tab.id}
                  data-active={active}
                  onClick={() => switchTab(tab.id)}
                  // Transition only while hovered, so switching tabs swaps styles instantly
                  className={`joined-tab group flex items-center gap-1.5 px-3.5 pt-2 text-sm font-medium whitespace-nowrap cursor-pointer ${
                    active
                      ? "pb-[9px] text-foreground"
                      : "pb-1 mb-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent hover:transition-colors"
                  }`}
                >
                  {tab.label}
                  {tabMetas.length > 1 && (
                    <span
                      onClick={(e) => closeTab(e, tab.id)}
                      className={`rounded p-0.5 transition-colors cursor-pointer ${
                        active
                          ? "text-muted-foreground hover:text-foreground hover:bg-accent"
                          : "text-transparent group-hover:text-muted-foreground hover:text-foreground! hover:bg-accent"
                      }`}
                    >
                      <X size={10} />
                    </span>
                  )}
                </button>
              );
            })}
          </div>
          <Button variant="ghost-muted" size="icon-xs" className="mb-1.5 mr-2" onClick={addTab} title="New query tab">
            <Plus />
          </Button>
        </div>

        {/* SQL editor */}
        <div className="flex-1 min-h-0">
          <SqlEditor
            onMount={(handle) => { editorRef.current = handle; }}
            initialValue={savedPayloads.get(activeId)?.sql ?? ""}
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
      <div className="flex-1 min-h-0 bg-card border border-border rounded-xl overflow-hidden">
        <QueryResults result={result} error={error} running={running} />
      </div>
    </div>
  );
}
