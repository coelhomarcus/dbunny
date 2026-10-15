import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightSpecialChars, drawSelection } from "@codemirror/view";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { appDarkThemeExtension } from "../lib/codemirror/theme";
import { sqlDialect } from "../lib/codemirror/sqlCompletion";

/** Read-only, syntax-highlighted SQL (function definitions, etc.). */
export default function SqlViewer({ value }: { value: string }) {
  const containerRef = useRef<HTMLDivElement>(null);

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
          sqlDialect.language,
          appDarkThemeExtension,
        ],
      }),
    });
    return () => view.destroy();
  }, [value]);

  return <div ref={containerRef} className="h-full" />;
}
