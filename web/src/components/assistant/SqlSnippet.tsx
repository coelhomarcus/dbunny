import { useEffect, useState } from "react";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { formatSqlCached } from "../../lib/formatSql";
import SqlViewer from "../SqlViewer";

// The compact viewer sets 12px text at a line height of 1.5 (18px) and 6px of padding above and below the lines
const LINE = 18;
const PADDING = 12;
const MAX_LINES = 12;
/** One line, and no longer than about the width of the panel. */
const SHORT = /^[^\n]{1,64}$/;

/**
 * A statement shown in the conversation: laid out the way the editor's formatter would (the assistant tends
 * to write it on one line), compact, and up to twelve lines tall (it scrolls past that). It is only how it is
 * shown; what runs is what the assistant wrote.
 */
export default function SqlSnippet({ sql }: { sql: string }) {
  const dialect = useSqlDialect();
  // The statement as written until its layout is ready, and the layout it was made for
  const [formatted, setFormatted] = useState<{ source: string; text: string } | null>(null);
  useEffect(() => {
    let current = true;
    formatSqlCached(sql, dialect).then((text) => current && setFormatted({ source: sql, text }));
    return () => {
      current = false;
    };
  }, [sql, dialect]);
  // A short statement stays as written: laid out it would take four lines for what fits in one
  const shown = SHORT.test(sql.trim()) ? sql.trim() : formatted && formatted.source === sql ? formatted.text.trimEnd() : sql;
  const lines = Math.min(MAX_LINES, Math.max(1, shown.split("\n").length));
  return (
    <div style={{ height: lines * LINE + PADDING }}>
      <SqlViewer value={shown} compact />
    </div>
  );
}
