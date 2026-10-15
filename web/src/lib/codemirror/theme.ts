import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const MONO = 'Menlo, "SF Mono", Consolas, "Courier New", monospace';

// Every colour comes from the CSS theme tokens in index.css, so the editor
// follows light/dark mode without being reconfigured.
export const appTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "14px",
    color: "var(--foreground)",
    backgroundColor: "var(--card)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: MONO, lineHeight: "1.5" },
  ".cm-content": { padding: "10px 0", caretColor: "var(--editor-cursor)" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--editor-cursor)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "var(--editor-selection)" },
  ".cm-selectionMatch": { backgroundColor: "var(--editor-selection)" },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: "transparent",
    outline: "1px solid var(--ring)",
  },
  ".cm-gutters": {
    backgroundColor: "var(--card)",
    color: "var(--editor-gutter)",
    border: "none",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 16px 0 20px" },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--editor-line-active)",
    color: "var(--editor-gutter-active)",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)",
    color: "var(--popover-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    overflow: "hidden",
    boxShadow: "0 8px 24px rgb(0 0 0 / 0.12)",
  },
  ".cm-tooltip-autocomplete > ul": { fontFamily: MONO, padding: "4px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 8px", borderRadius: "5px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--accent)",
    color: "var(--foreground)",
  },
  ".cm-completionDetail": { color: "var(--muted-foreground)", fontStyle: "normal" },
  ".cm-completionMatchedText": { textDecoration: "none", color: "var(--primary)", fontWeight: "600" },
  ".cm-panels": { backgroundColor: "var(--popover)", color: "var(--foreground)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--border)" },
  ".cm-panel input, .cm-panel button": { fontSize: "12px" },
  ".cm-textfield": {
    backgroundColor: "var(--card)",
    color: "var(--foreground)",
    border: "1px solid var(--input)",
    borderRadius: "6px",
  },
  ".cm-button": {
    backgroundImage: "none",
    backgroundColor: "var(--secondary)",
    color: "var(--foreground)",
    border: "1px solid var(--input)",
    borderRadius: "6px",
  },
  ".cm-searchMatch": { backgroundColor: "color-mix(in srgb, var(--warning) 30%, transparent)" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "color-mix(in srgb, var(--primary) 35%, transparent)" },
});

const appHighlight = HighlightStyle.define([
  { tag: t.keyword, color: "var(--syntax-keyword)" },
  { tag: [t.typeName, t.standard(t.name)], color: "var(--syntax-type)" },
  { tag: [t.string, t.special(t.string)], color: "var(--syntax-string)" },
  { tag: [t.number, t.bool, t.null], color: "var(--syntax-number)" },
  { tag: [t.lineComment, t.blockComment], color: "var(--syntax-comment)" },
  { tag: [t.operator, t.punctuation], color: "var(--foreground)" },
  { tag: t.special(t.name), color: "var(--syntax-special)" },
]);

export const appThemeExtension = [appTheme, syntaxHighlighting(appHighlight)];
