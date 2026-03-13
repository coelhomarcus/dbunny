import { useState, useCallback, useRef, useEffect } from "react";
import Editor, { type OnMount, type BeforeMount } from "@monaco-editor/react";
import { api } from "../lib/api";
import type { QueryResult } from "@/types";
import { useSqlAutocomplete } from "../hooks/useSqlAutocomplete";
import QueryResults from "../components/QueryResults";
import { Plus, X, Upload, Download, Loader } from "lucide-react";

type TabMeta = {
  id: number;
  label: string;
};

type TabPayload = {
  sql: string;
  result: QueryResult | null;
  error: string | null;
};

// Module-level — survives navigation (component unmount/remount)
let nextId = 2;
let savedTabMetas: TabMeta[] = [{ id: 1, label: "Query 1" }];
let savedActiveId = 1;
const savedPayloads: Map<number, TabPayload> = new Map([
  [1, { sql: "SELECT 1;", result: null, error: null }],
]);

export default function QueryEditor() {
  const [tabMetas, setTabMetas] = useState<TabMeta[]>(savedTabMetas);
  const [activeId, setActiveId] = useState(savedActiveId);
  const initial = savedPayloads.get(savedActiveId)!;
  const [result, setResult] = useState<QueryResult | null>(initial.result);
  const [error, setError] = useState<string | null>(initial.error);
  const [running, setRunning] = useState(false);

  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const activeIdRef = useRef(activeId);
const { registerCompletionProvider } = useSqlAutocomplete();

  useEffect(() => {
    activeIdRef.current = activeId;
  }, [activeId]);

  // Save editor SQL on unmount so it's available when navigating back
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

    if (path) {
      await writeTextFile(path, sql);
    }
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

  const runQuery = useCallback(async () => {
    const editor = editorRef.current;
    if (!editor) return;

    const selection = editor.getSelection();
    let sql: string;
    if (selection && !selection.isEmpty()) {
      sql = editor.getModel()?.getValueInRange(selection) ?? "";
    } else {
      sql = editor.getValue();
    }

    if (!sql.trim()) return;

    setRunning(true);
    setError(null);
    setResult(null);

    const delay = new Promise<void>((r) => setTimeout(r, 400));
    try {
      const [res] = await Promise.all([api.query(sql), delay]);
      setResult(res);
      const payload = savedPayloads.get(activeIdRef.current);
      if (payload) { payload.result = res; payload.error = null; }
    } catch (err) {
      await delay;
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : "Query failed";
      setError(msg);
      const payload = savedPayloads.get(activeIdRef.current);
      if (payload) { payload.result = null; payload.error = msg; }
    } finally {
      setRunning(false);
    }
  }, []);

  const handleBeforeMount: BeforeMount = (monaco) => {
    monaco.editor.defineTheme("app-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": "#09090b",
        "editor.lineHighlightBackground": "#18181b",
        "editorLineNumber.foreground": "#3f3f46",
        "editorLineNumber.activeForeground": "#71717a",
        "editor.selectionBackground": "#27272a",
        "editorGutter.background": "#09090b",
        "scrollbar.shadow": "#09090b",
        "editorWidget.background": "#18181b",
        "editorWidget.border": "#27272a",
        "input.background": "#18181b",
        "input.border": "#27272a",
      },
    });
    registerCompletionProvider(monaco);
  };

  const handleEditorMount: OnMount = (editor) => {
    editorRef.current = editor;
    const payload = savedPayloads.get(activeId);
    editor.setValue(payload?.sql ?? "");
    editor.addAction({
      id: "run-query",
      label: "Run Query",
      keybindings: [2048 | 3],
      run: () => runQuery(),
    });
  };

  return (
    <div className="h-full flex flex-col">
      {/* Tabs */}
      <div className="flex items-center border-b border-zinc-800 bg-zinc-950 shrink-0">
        <div className="flex items-center flex-1 overflow-x-auto overflow-y-hidden">
          {tabMetas.map((tab) => (
            <button
              key={tab.id}
              onClick={() => switchTab(tab.id)}
              className={`group flex items-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors border-b-2 -mb-px whitespace-nowrap ${
                activeId === tab.id
                  ? "text-white border-blue-500"
                  : "text-zinc-500 border-transparent hover:text-zinc-300"
              }`}
            >
              {tab.label}
              {tabMetas.length > 1 && (
                <span
                  onClick={(e) => closeTab(e, tab.id)}
                  className={`rounded p-0.5 transition-colors cursor-pointer ${
                    activeId === tab.id
                      ? "text-zinc-400 hover:text-white hover:bg-zinc-700"
                      : "text-transparent group-hover:text-zinc-500 hover:text-white! hover:bg-zinc-700"
                  }`}
                >
                  <X size={10} />
                </span>
              )}
            </button>
          ))}
        </div>
        <button
          onClick={addTab}
          className="px-2 py-2 text-zinc-600 hover:text-zinc-300 transition-colors shrink-0"
          title="New query tab"
        >
          <Plus size={14} />
        </button>
      </div>

      {/* Editor */}
      <div className="flex-1 min-h-0 border-b border-zinc-800">
        <Editor
          defaultLanguage="sql"
          theme="app-dark"
          beforeMount={handleBeforeMount}
          onMount={handleEditorMount}
          options={{
            minimap: { enabled: false },
            fontSize: 14,
            lineNumbers: "on",
            scrollBeyondLastLine: false,
            wordWrap: "on",
            padding: { top: 8, bottom: 8 },
            renderLineHighlight: "gutter",
            automaticLayout: true,
          }}
        />
      </div>

      {/* Toolbar */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-zinc-800 bg-zinc-900">
        <button
          onClick={runQuery}
          disabled={running}
          className="px-4 py-1.5 bg-green-600 hover:bg-green-500 disabled:bg-zinc-700 text-white text-xs font-medium rounded transition-colors relative"
        >
          <span className={`transition-opacity duration-200 ${running ? "opacity-0" : "opacity-100"}`}>
            Run (Ctrl+Enter)
          </span>
          <Loader className={`w-3.5 h-3.5 animate-spin absolute inset-0 m-auto transition-opacity duration-200 ${running ? "opacity-100" : "opacity-0"}`} />
        </button>
        {result && (
          <span className="text-xs text-zinc-500">
            {result.rowsAffected != null
              ? `${result.rowsAffected} rows affected in ${result.duration}ms`
              : `${result.rowCount} rows in ${result.duration}ms`}
          </span>
        )}
        <div className="flex items-center gap-1 ml-auto">
          <button
            onClick={importQuery}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 text-xs rounded transition-colors"
            title="Import .sql file"
          >
            <Upload size={13} />
            Import
          </button>
          <button
            onClick={exportQuery}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 text-xs rounded transition-colors"
            title="Export as .sql file"
          >
            <Download size={13} />
            Export
          </button>
        </div>
      </div>
{/* Results */}
      <QueryResults result={result} error={error} running={running} />
    </div>
  );
}
