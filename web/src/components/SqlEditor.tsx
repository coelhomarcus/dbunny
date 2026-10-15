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
import { appDarkThemeExtension } from "../lib/codemirror/theme";
import { sqlCompletion, sqlDialect } from "../lib/codemirror/sqlCompletion";
import type { CompletionSchema } from "@/types";

export type SqlEditorHandle = {
  getValue(): string;
  /** Replaces the whole document and resets undo history. */
  setValue(sql: string): void;
  /** Selected text if there is a selection, otherwise the whole document. */
  getSelectionOrValue(): string;
};

type SqlEditorProps = {
  initialValue: string;
  /** Database catalog used for table/column/function autocomplete. */
  schema: CompletionSchema | null;
  /** Called once the editor exists. The handle stays usable after unmount (reads the last state). */
  onMount: (handle: SqlEditorHandle) => void;
  onRun: () => void;
};

export default function SqlEditor({ initialValue, schema, onMount, onRun }: SqlEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const completionRef = useRef(new Compartment());
  const schemaRef = useRef(schema);
  // Keymaps are created once, so always call the latest onRun
  const onRunRef = useRef(onRun);

  useEffect(() => {
    onRunRef.current = onRun;
  }, [onRun]);

  useEffect(() => {
    const createState = (doc: string) =>
      EditorState.create({ doc, extensions: buildExtensions() });

    const buildExtensions = (): Extension[] => [
      Prec.highest(
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              onRunRef.current();
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
      sqlDialect.language,
      completionRef.current.of(sqlCompletion(schemaRef.current)),
      appDarkThemeExtension,
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
