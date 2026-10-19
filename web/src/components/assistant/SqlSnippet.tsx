import SqlViewer from "../SqlViewer";

/** One line of the viewer, and the room above and below the statement. */
const LINE = 21;
const BREATHING_ROOM = 22;

/** A statement shown in the conversation: up to eight lines tall (it scrolls past that), with room around it. */
export default function SqlSnippet({ sql }: { sql: string }) {
  const lines = Math.min(8, Math.max(1, sql.split("\n").length));
  return (
    <div className="py-2.5" style={{ height: lines * LINE + BREATHING_ROOM }}>
      <SqlViewer value={sql} />
    </div>
  );
}
