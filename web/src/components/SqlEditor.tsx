import { useEffect, useRef } from "react";
import { Compartment, EditorState, Prec, type Extension } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLineGutter,
  highlightSpecialChars,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { indentOnInput, bracketMatching } from "@codemirror/language";
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { appThemeExtension } from "../lib/codemirror/theme";
import { sqlCompletion, sqlDialect } from "../lib/codemirror/sqlCompletion";
import { currentStatementHighlight, currentStatementRange } from "../lib/codemirror/currentStatement";
import { formatSql } from "../lib/formatSql";
import type { CompletionSchema } from "@/types";

/** "current": the selection, or the statement at the cursor; "all": the whole script. */
export type RunScope = "current" | "all";

export type SqlEditorHandle = {
  getValue(): string;
  /** Replaces the whole document and resets undo history. */
  setValue(sql: string): void;
  /** Selected text if there is a selection, otherwise the whole document. */
  getSelectionOrValue(): string;
  /** The selection, or the statement the cursor is in (empty when there's none). */
  getCurrentStatement(): string;
  /** Formats the selection, or the whole document, as one undoable edit. */
  format(): Promise<void>;
};

type SqlEditorProps = {
  initialValue: string;
  /** Database catalog used for table/column/function autocomplete. */
  schema: CompletionSchema | null;
  /** Called once the editor exists. The handle stays usable after unmount (reads the last state). */
  onMount: (handle: SqlEditorHandle) => void;
  onRun: (scope: RunScope) => void;
  /** Called with the new text after every edit. */
  onChange?: (sql: string) => void;
  /** Called when the SQL couldn't be formatted (usually a syntax the formatter doesn't know). */
  onFormatError?: (message: string) => void;
};

/** Formats the selection, or the whole document; one transaction, so a single undo restores it. */
async function formatView(view: EditorView, onError?: (message: string) => void) {
  const { from, to, empty } = view.state.selection.main;
  const range = empty ? { from: 0, to: view.state.doc.length } : { from, to };
  const before = view.state.sliceDoc(range.from, range.to);
  if (!before.trim()) return;
  let after: string;
  try {
    after = await formatSql(before);
  } catch (err) {
    onError?.(err instanceof Error ? err.message.split("\n")[0] : "Couldn't format this SQL");
    return;
  }
  // The text may have changed while the formatter loaded
  if (view.state.sliceDoc(range.from, range.to) !== before || after === before) return;
  view.dispatch({
    changes: { ...range, insert: after },
    selection: empty ? { anchor: Math.min(view.state.selection.main.head, after.length) } : { anchor: range.from, head: range.from + after.length },
    scrollIntoView: true,
  });
}

export default function SqlEditor({ initialValue, schema, onMount, onRun, onChange, onFormatError }: SqlEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const completionRef = useRef(new Compartment());
  const schemaRef = useRef(schema);
  // Keymaps are created once, so always call the latest onRun
  const onRunRef = useRef(onRun);

  const onChangeRef = useRef(onChange);
  const onFormatErrorRef = useRef(onFormatError);

  useEffect(() => {
    onRunRef.current = onRun;
    onChangeRef.current = onChange;
    onFormatErrorRef.current = onFormatError;
  }, [onRun, onChange, onFormatError]);

  useEffect(() => {
    const createState = (doc: string) =>
      EditorState.create({ doc, extensions: buildExtensions() });

    const buildExtensions = (): Extension[] => [
      Prec.highest(
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              onRunRef.current("current");
              return true;
            },
          },
          {
            key: "Shift-Mod-Enter",
            run: () => {
              onRunRef.current("all");
              return true;
            },
          },
          {
            key: "Shift-Alt-f",
            run: (view) => {
              formatView(view, onFormatErrorRef.current);
              return true;
            },
          },
        ]),
      ),
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      autocompletion({ icons: true }),
      rectangularSelection(),
      crosshairCursor(),
      highlightSelectionMatches(),
      search({ top: true }),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      EditorView.lineWrapping,
      currentStatementHighlight,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) onChangeRef.current?.(update.state.doc.toString());
      }),
      sqlDialect.language,
      completionRef.current.of(sqlCompletion(schemaRef.current)),
      appThemeExtension,
    ];

    const view = new EditorView({
      parent: containerRef.current!,
      state: createState(initialValue),
    });
    viewRef.current = view;

    onMount({
      getValue() {
        return viewRef.current?.state.doc.toString() ?? "";
      },
      setValue(sql) {
        viewRef.current?.setState(createState(sql));
      },
      getSelectionOrValue() {
        const state = viewRef.current?.state;
        if (!state) return "";
        const { from, to, empty } = state.selection.main;
        return empty ? state.doc.toString() : state.sliceDoc(from, to);
      },
      getCurrentStatement() {
        const state = viewRef.current?.state;
        if (!state) return "";
        const range = currentStatementRange(state);
        return range ? state.sliceDoc(range.from, range.to) : "";
      },
      async format() {
        const view = viewRef.current;
        if (view) await formatView(view, onFormatErrorRef.current);
        view?.focus();
      },
    });

    return () => {
      // Keep viewRef pointing at the destroyed view: its state stays readable,
      // so a parent unmount cleanup can still grab the final SQL through the handle.
      view.destroy();
    };
    // initialValue/onMount are only used on mount; later changes go through setValue
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Swap autocomplete sources when the catalog (re)loads, without touching the document
  useEffect(() => {
    schemaRef.current = schema;
    viewRef.current?.dispatch({
      effects: completionRef.current.reconfigure(sqlCompletion(schema)),
    });
  }, [schema]);

  return <div ref={containerRef} className="h-full" />;
}
