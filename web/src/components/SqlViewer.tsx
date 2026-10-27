import { useEffect, useRef } from "react";
import { EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightSpecialChars, drawSelection } from "@codemirror/view";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { appThemeExtension } from "../lib/codemirror/theme";
import { languageFor } from "../lib/sql/dialects";
import { useConnection } from "../contexts/useConnection";

/** The compact look, for a statement shown inside the conversation: smaller text, little padding, no line numbers. */
const compactTheme = EditorView.theme({
  "&": { fontSize: "12px" },
  ".cm-content": { padding: "6px 12px" },
});

/** Read-only, syntax-highlighted SQL (function definitions, etc.); `compact` is for a statement in the conversation. */
export default function SqlViewer({ value, compact = false }: { value: string; compact?: boolean }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const engine = useConnection().connectionInfo?.engine ?? "postgres";

  useEffect(() => {
    const view = new EditorView({
      parent: containerRef.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          // Keep the content focusable so text can be selected/copied and searched
          EditorView.contentAttributes.of({ tabindex: "0" }),
          ...(compact ? [] : [lineNumbers()]),
          highlightSpecialChars(),
          drawSelection(),
          highlightSelectionMatches(),
          search({ top: true }),
          keymap.of(searchKeymap),
          languageFor(engine).language,
          appThemeExtension,
          // Over the app theme, which sets the editor's own sizes
          ...(compact ? [Prec.high(compactTheme)] : []),
        ],
      }),
    });
    return () => view.destroy();
  }, [value, engine, compact]);

  return <div ref={containerRef} className="h-full" />;
}
