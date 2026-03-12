import { useState, useCallback, useRef } from "react";
import Editor, { type OnMount, type BeforeMount } from "@monaco-editor/react";
import { api } from "../lib/api";
import type { QueryResult } from "@/types";
import { useSqlAutocomplete } from "../hooks/useSqlAutocomplete";
import QueryResults from "../components/QueryResults";

export default function QueryEditor() {
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

  const { registerCompletionProvider } = useSqlAutocomplete();

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

    try {
      const res = await api.query(sql);
      setResult(res);
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Query failed");
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

    editor.addAction({
      id: "run-query",
      label: "Run Query",
      keybindings: [2048 | 3], // Ctrl+Enter
      run: () => runQuery(),
    });
  };

  return (
    <div className="h-full flex flex-col">
      {/* Editor */}
      <div className="flex-1 min-h-0 border-b border-zinc-800">
        <Editor
          defaultLanguage="sql"
          defaultValue="SELECT 1;"
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
          className="px-4 py-1.5 bg-green-600 hover:bg-green-500 disabled:bg-zinc-700 text-white text-xs font-medium rounded transition-colors"
        >
          {running ? "Running..." : "Run (Ctrl+Enter)"}
        </button>
        {result && (
          <span className="text-xs text-zinc-500">
            {result.rowCount} rows in {result.duration}ms
          </span>
        )}
      </div>

      {/* Results */}
      <QueryResults result={result} error={error} running={running} />
    </div>
  );
}
