import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

export const appDarkTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      fontSize: "14px",
      color: "#d4d4d4",
      backgroundColor: "#18181b",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": {
      fontFamily: 'Menlo, "SF Mono", Consolas, "Courier New", monospace',
      lineHeight: "1.5",
    },
    ".cm-content": { padding: "10px 0", caretColor: "#aeafad" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#aeafad" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      { backgroundColor: "#2d2d33" },
    ".cm-selectionMatch": { backgroundColor: "#2d2d3399" },
    ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
      backgroundColor: "transparent",
      outline: "1px solid #52525b",
    },
    ".cm-gutters": {
      backgroundColor: "#18181b",
      color: "#3f3f46",
      border: "none",
    },
    ".cm-lineNumbers .cm-gutterElement": { padding: "0 16px 0 20px" },
    ".cm-activeLineGutter": { backgroundColor: "#1e1e22", color: "#71717a" },
    ".cm-tooltip": {
      backgroundColor: "#1e1e22",
      border: "1px solid #27272a",
      borderRadius: "6px",
      overflow: "hidden",
    },
    ".cm-tooltip-autocomplete > ul": { fontFamily: 'Menlo, "SF Mono", Consolas, "Courier New", monospace' },
    ".cm-tooltip-autocomplete > ul > li": { padding: "2px 8px" },
    ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "#2d2d33",
      color: "#ffffff",
    },
    ".cm-completionMatchedText": { textDecoration: "none", color: "#60a5fa", fontWeight: "600" },
    ".cm-panels": { backgroundColor: "#1e1e22", color: "#d4d4d4" },
    ".cm-panels.cm-panels-top": { borderBottom: "1px solid #27272a" },
    ".cm-panels.cm-panels-bottom": { borderTop: "1px solid #27272a" },
    ".cm-panel input, .cm-panel button": { fontSize: "12px" },
    ".cm-textfield": { backgroundColor: "#18181b", border: "1px solid #27272a", borderRadius: "4px" },
    ".cm-button": { backgroundImage: "none", backgroundColor: "#27272a", border: "1px solid #3f3f46", borderRadius: "4px" },
    ".cm-searchMatch": { backgroundColor: "#515c6a80" },
    ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#62331c" },
  },
  { dark: true },
);

// VS Code "Dark+" token colours
export const appDarkHighlight = HighlightStyle.define([
  { tag: t.keyword, color: "#569cd6" },
  { tag: [t.typeName, t.standard(t.name)], color: "#569cd6" },
  { tag: [t.string, t.special(t.string)], color: "#ce9178" },
  { tag: [t.number, t.bool, t.null], color: "#b5cea8" },
  { tag: [t.lineComment, t.blockComment], color: "#6a9955" },
  { tag: [t.operator, t.punctuation], color: "#d4d4d4" },
  { tag: t.special(t.name), color: "#9cdcfe" },
]);

export const appDarkThemeExtension = [appDarkTheme, syntaxHighlighting(appDarkHighlight)];
