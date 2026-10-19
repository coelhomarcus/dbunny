import SqlViewer from "../SqlViewer";

// The editor theme (lib/codemirror/theme.ts) sets 14px text at a line height of 1.5 and 10px of padding above and below the lines
const LINE = 21;
const THEME_PADDING = 20;
/** Extra room around the viewer, above and below. */
const MARGIN = 6;

/** A statement shown in the conversation: up to eight lines tall (it scrolls past that), with room around it. */
export default function SqlSnippet({ sql }: { sql: string }) {
  const lines = Math.min(8, Math.max(1, sql.split("\n").length));
  return (
    <div style={{ height: lines * LINE + THEME_PADDING + 2 * MARGIN, paddingBlock: MARGIN }}>
      <SqlViewer value={sql} />
    </div>
  );
}
