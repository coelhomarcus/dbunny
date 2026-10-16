import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightSpecialChars, drawSelection } from "@codemirror/view";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { appThemeExtension } from "../lib/codemirror/theme";
import { dialectFor } from "../lib/codemirror/sqlCompletion";
import { useSqlDialect } from "../hooks/useSqlDialect";

/** Read-only, syntax-highlighted SQL (function definitions, etc.). */
export default function SqlViewer({ value }: { value: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dialect = useSqlDialect();

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
          lineNumbers(),
          highlightSpecialChars(),
          drawSelection(),
          highlightSelectionMatches(),
          search({ top: true }),
          keymap.of(searchKeymap),
          dialectFor(dialect).language,
          appThemeExtension,
        ],
      }),
    });
    return () => view.destroy();
  }, [value, dialect]);

  return <div ref={containerRef} className="h-full" />;
}
