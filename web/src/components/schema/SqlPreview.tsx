import { language } from "@twinkleplop/sql";

const highlight = language();

/** Read-only SQL, colored with the same tokens as the query editor. */
export default function SqlPreview({ source }: { source: string }) {
  return <div className="sql-preview" dangerouslySetInnerHTML={{ __html: highlight(source) }} />;
}
