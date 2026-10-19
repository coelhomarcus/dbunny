import { useState } from "react";
import { Ban, Check, Database, Loader2, Network, Table2, TriangleAlert } from "lucide-react";
import { parseToolOutput, toolLabel, type ToolItem } from "../../lib/assistantConversation";
import type { Decision } from "../../types/assistant";
import ApprovalButtons from "./ApprovalButtons";
import ResultTable from "./ResultTable";
import SqlSnippet from "./SqlSnippet";

/** The text the assistant is given when the person turns a query down. */
const DECLINED = "The user declined to run this query. Ask them how to continue.";
const PREVIEW_ROWS = 10;

/** One thing the assistant did with the database: a line for what it looked at, a card for a query. */
export default function ToolCard({ item, onDecide }: { item: ToolItem; onDecide: (approvalId: string, decision: Decision) => void }) {
  if (item.tool === "run_query" || item.tool === "explain_query") return <QueryCard item={item} onDecide={onDecide} />;
  const Icon = item.tool === "relationships" ? Network : item.tool === "describe_table" ? Table2 : Database;
  const running = item.status === "pending" || item.status === "in_progress";
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      {running ? <Loader2 size={13} className="animate-spin" /> : item.status === "failed" ? <TriangleAlert size={13} /> : <Icon size={13} />}
      <span>{toolLabel(item)}</span>
    </div>
  );
}

function QueryCard({ item, onDecide }: { item: ToolItem; onDecide: (approvalId: string, decision: Decision) => void }) {
  const [showAll, setShowAll] = useState(false);
  const sql = typeof item.input.sql === "string" ? item.input.sql : "";
  const rows = item.output ? parseToolOutput(item.output) : null;
  const waiting = item.approval !== undefined;
  const running = !waiting && (item.status === "pending" || item.status === "in_progress");
  const declined = item.status === "failed" && item.output === DECLINED;

  // Present tense until it is done
  const label = waiting || running ? (item.tool === "explain_query" ? "Explain a query" : "Run a query") : toolLabel(item);
  let state: string;
  if (waiting) state = "Waiting for approval";
  else if (running) state = "Running…";
  else if (declined) state = "Declined";
  else if (item.status === "failed") state = "Error";
  else if (rows) state = rows.valuesHidden ? `${rows.rowCount} rows (values hidden)` : `${rows.rowCount} rows${rows.truncated ? " (cut)" : ""}`;
  else state = "Done";

  const shown = rows && !rows.valuesHidden ? (showAll ? rows.rows : rows.rows.slice(0, PREVIEW_ROWS)) : [];
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-card text-xs">
      <div className="flex items-center gap-2 px-2.5 py-1.5">
        {waiting || running ? <Loader2 size={13} className="animate-spin text-muted-foreground" /> : declined ? <Ban size={13} className="text-muted-foreground" /> : item.status === "failed" ? <TriangleAlert size={13} className="text-destructive-foreground" /> : <Check size={13} className="text-success-foreground" />}
        <span className="font-medium">{label}</span>
        <span className="flex-1" />
        <span className="text-muted-foreground">{state}</span>
      </div>
      {sql && (
        <div className="border-t border-border">
          <SqlSnippet sql={sql} />
        </div>
      )}
      {item.approval && (
        <div className="border-t border-border px-2.5 py-2">
          <ApprovalButtons onDecide={(decision) => onDecide(item.approval!.id, decision)} />
        </div>
      )}
      {item.status === "failed" && item.output && !declined && (
        <p className="border-t border-border px-2.5 py-1.5 whitespace-pre-wrap text-destructive-foreground">{item.output}</p>
      )}
      {rows && shown.length > 0 && (
        <div className="border-t border-border">
          <ResultTable
            columns={rows.columns.map((column) => column.name)}
            rows={shown.map((row) => row.map((value) => (value === null ? <span className="text-muted-foreground">NULL</span> : typeof value === "string" ? value : JSON.stringify(value))))}
          />
          {rows.rows.length > PREVIEW_ROWS && !showAll && (
            <button type="button" className="w-full cursor-pointer border-t border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent" onClick={() => setShowAll(true)}>
              Show all {rows.rows.length} rows
            </button>
          )}
        </div>
      )}
    </div>
  );
}
