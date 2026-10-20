import { useEffect, useImperativeHandle, useLayoutEffect, useRef, type RefObject } from "react";
import { json } from "@codemirror/lang-json";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers } from "@codemirror/view";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { appThemeExtension } from "../../lib/codemirror/theme";
import { t } from "../../i18n";

export type JsonValueEditorHandle = {
  /** Pretty-prints the document. Returns an error message when it isn't JSON. */
  format(): string | null;
};

/** Editable JSON, colored with the same theme as the query editor. Starts from `initialValue`. */
export default function JsonValueEditor({
  initialValue,
  onChange,
  onApply,
  handleRef,
}: {
  initialValue: string;
  onChange: (text: string) => void;
  onApply: () => void;
  handleRef: RefObject<JsonValueEditorHandle | null>;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const onChangeRef = useRef(onChange);
  const onApplyRef = useRef(onApply);
  // The editor is made once, so its listeners read the latest callbacks from refs, kept up to date after each render
  useLayoutEffect(() => {
    onChangeRef.current = onChange;
    onApplyRef.current = onApply;
  });
  const viewRef = useRef<EditorView | null>(null);

  useImperativeHandle(handleRef, () => ({
    format() {
      const view = viewRef.current;
      if (!view) return null;
      const before = view.state.doc.toString();
      let after: string;
      try {
        after = JSON.stringify(JSON.parse(before), null, 2);
      } catch (err) {
        return err instanceof Error ? err.message : t("table.cellEditorDialog.invalidJson");
      }
      if (after !== before) {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: after } });
      }
      return null;
    },
  }));

  useEffect(() => {
    const parent = hostRef.current;
    if (!parent) return;
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: initialValue,
        extensions: [
          lineNumbers(),
          history(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          json(),
          keymap.of([indentWithTab, ...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap]),
          EditorView.domEventHandlers({
            keydown(event) {
              if ((event.metaKey || event.ctrlKey) && (event.key === "Enter" || event.key === "s")) {
                event.preventDefault();
                event.stopPropagation();
                onApplyRef.current();
                return true;
              }
              return false;
            },
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChangeRef.current(update.state.doc.toString());
          }),
          appThemeExtension,
          EditorView.theme({
            "&": { height: "18rem", fontSize: "12px" },
            ".cm-scroller": { overflow: "auto" },
          }),
        ],
      }),
    });
    viewRef.current = view;
    view.focus();
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Mounted only while the dialog is open, so the first value is the one to edit
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={hostRef} className="overflow-hidden rounded-lg border border-input" />;
}
