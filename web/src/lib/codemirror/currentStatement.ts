import { RangeSetBuilder, StateField, type EditorState } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { splitStatements, statementAt, type StatementRange } from "../sqlStatements";

const lineMark = Decoration.line({ class: "cm-current-statement" });

/** The statement Cmd+Enter would run: the selection's text, or the statement at the cursor. */
export function currentStatementRange(state: EditorState): StatementRange | null {
  const { from, to, empty } = state.selection.main;
  if (!empty) return { from, to };
  return statementAt(splitStatements(state.doc.toString()), state.selection.main.head);
}

function marks(state: EditorState): DecorationSet {
  // With one statement (or a selection) there's nothing to tell apart
  if (!state.selection.main.empty) return Decoration.none;
  const ranges = splitStatements(state.doc.toString());
  if (ranges.length < 2) return Decoration.none;
  const range = statementAt(ranges, state.selection.main.head);
  if (!range) return Decoration.none;
  const builder = new RangeSetBuilder<Decoration>();
  const last = state.doc.lineAt(range.to).number;
  for (let n = state.doc.lineAt(range.from).number; n <= last; n++) {
    const line = state.doc.line(n);
    builder.add(line.from, line.from, lineMark);
  }
  return builder.finish();
}

/** Tints the lines of the statement Cmd+Enter would run, when the script has several. */
export const currentStatementHighlight = [
  StateField.define<DecorationSet>({
    create: marks,
    update: (value, tr) => (tr.docChanged || tr.selection ? marks(tr.state) : value),
    provide: (field) => EditorView.decorations.from(field),
  }),
  EditorView.baseTheme({
    ".cm-current-statement": {
      backgroundColor: "color-mix(in srgb, var(--primary) 6%, transparent)",
      boxShadow: "inset 2px 0 0 color-mix(in srgb, var(--primary) 55%, transparent)",
    },
  }),
];
