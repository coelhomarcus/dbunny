import { useState } from "react";
import { Ban, Brain, ChevronDown, ChevronUp, Database, Gauge, ListChecks, Network, Table2, type LucideIcon } from "lucide-react";
import { parseToolOutput, type Item, type ToolItem } from "../../lib/assistantConversation";
import { isDeclined, stepsSummary, toolDescription, toolTitle, type Block } from "../../lib/assistantTurns";
import type { ApprovalRequest, Decision } from "../../types/assistant";
import {
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
  type TraceStatus,
} from "../chat/Trace";
import ApprovalButtons from "./ApprovalButtons";
import MarkdownView from "./MarkdownView";
import ResultTable from "./ResultTable";
import SqlSnippet from "./SqlSnippet";
import { useT } from "../../i18n";

const PREVIEW_ROWS = 10;

const ICONS: Record<string, LucideIcon> = { list_tables: Database, describe_table: Table2, relationships: Network, run_query: Database, explain_query: Gauge };

type StepsBlock = Extract<Block, { kind: "steps" }>;

/**
 * A run of steps. While it goes on its steps are lines of a trace; once it is over it is folded to a line
 * that says how long it took and how many there were, and that opens on the same lines.
 */
export default function StepsView({
  block,
  working,
  onDecide,
}: {
  block: StepsBlock;
  /** The turn is still running and these are its last steps. */
  working: boolean;
  onDecide: (approvalId: string, decision: Decision) => void;
}) {
  const lines = (
    <Trace>
      {block.items.map((item, index) => (
        <Step key={item.kind === "tool" ? item.id : item.kind === "approval" ? item.approval.id : index} item={item} thinking={working && index === block.items.length - 1} onDecide={onDecide} />
      ))}
    </Trace>
  );
  if (!block.folded) return lines;
  return (
    <TraceCollapsible>
      <TraceCollapsibleTrigger>
        <TraceLine>
          <TraceIcon>
            <ListChecks />
          </TraceIcon>
          <TraceTitle>{stepsSummary(block.items, block.end)}</TraceTitle>
        </TraceLine>
      </TraceCollapsibleTrigger>
      <TraceCollapsibleContent>{lines}</TraceCollapsibleContent>
    </TraceCollapsible>
  );
}

function Step({ item, thinking, onDecide }: { item: Item; thinking: boolean; onDecide: (approvalId: string, decision: Decision) => void }) {
  if (item.kind === "thought") return <Thought text={item.text} thinking={thinking} />;
  if (item.kind === "approval") return <QueryRow sql={item.approval.sql} approval={item.approval} analyze={item.approval.analyze} onDecide={onDecide} />;
  if (item.kind !== "tool") return null;
  if (item.tool === "run_query" || item.tool === "explain_query") {
    return <QueryRow tool={item} sql={typeof item.input.sql === "string" ? item.input.sql : ""} approval={item.approval} analyze={item.tool === "explain_query"} onDecide={onDecide} />;
  }
  const Icon = ICONS[item.tool] ?? Database;
  const status: TraceStatus | undefined = item.status === "pending" || item.status === "in_progress" ? "running" : item.status === "failed" ? "error" : undefined;
  return (
    <TraceLine status={status}>
      <TraceIcon>
        <Icon />
      </TraceIcon>
      <TraceTitle>{toolTitle(item)}</TraceTitle>
      <TraceDescription>{toolDescription(item)}</TraceDescription>
    </TraceLine>
  );
}

/** What the assistant thought: a line that opens on it, open and shimmering while it is still thinking. */
function Thought({ text, thinking }: { text: string; thinking: boolean }) {
  const t = useT();
  // The reader's click wins over opening while it streams
  const [choice, setChoice] = useState<boolean | undefined>();
  return (
    <TraceCollapsible open={choice ?? thinking} onOpenChange={setChoice}>
      <TraceCollapsibleTrigger>
        <TraceLine status={thinking ? "running" : undefined}>
          <TraceIcon>
            <Brain />
          </TraceIcon>
          <TraceTitle>{thinking ? t("assistant.messageList.thinking") : t("assistant.steps.thought")}</TraceTitle>
        </TraceLine>
      </TraceCollapsibleTrigger>
      <TraceCollapsibleContent>
        <MarkdownView source={text} className="text-xs text-muted-foreground" />
      </TraceCollapsibleContent>
    </TraceCollapsible>
  );
}

/**
 * A query: open on its statement and the buttons while it waits for the person or runs, and a line that
 * says what it found once it is done, which opens on the statement and the rows.
 */
function QueryRow({
  tool,
  sql,
  approval,
  analyze,
  onDecide,
}: {
  tool?: ToolItem;
  sql: string;
  approval?: ApprovalRequest;
  analyze: boolean;
  onDecide: (approvalId: string, decision: Decision) => void;
}) {
  const [choice, setChoice] = useState<boolean | undefined>();
  const t = useT();
  const [showAll, setShowAll] = useState(false);
  const [rowsOpen, setRowsOpen] = useState(false);
  const waiting = approval !== undefined;
  const running = !waiting && tool !== undefined && (tool.status === "pending" || tool.status === "in_progress");
  const declined = tool !== undefined && isDeclined(tool);
  const failed = tool?.status === "failed" && !declined;
  const rows = tool?.output ? parseToolOutput(tool.output) : null;
  const canShowRows = rows !== null && !rows.valuesHidden && rows.rows.length > 0;
  const shown = rows && canShowRows ? (showAll ? rows.rows : rows.rows.slice(0, PREVIEW_ROWS)) : [];

  const title = waiting ? (analyze ? t("assistant.steps.runToExplain") : t("assistant.steps.runAQuery")) : tool ? toolTitle(tool) : t("assistant.steps.runAQuery");
  const status: TraceStatus | undefined = running ? "running" : failed ? "error" : undefined;
  const Icon = declined ? Ban : analyze ? Gauge : Database;

  return (
    <TraceCollapsible open={choice ?? (waiting || running)} onOpenChange={setChoice}>
      <TraceCollapsibleTrigger>
        <TraceLine status={status}>
          <TraceIcon>
            <Icon />
          </TraceIcon>
          <TraceTitle>{title}</TraceTitle>
          <TraceDescription>{waiting ? t("assistant.steps.waitingForApproval") : tool ? toolDescription(tool) : ""}</TraceDescription>
        </TraceLine>
      </TraceCollapsibleTrigger>
      <TraceCollapsibleContent className="space-y-2 pr-1">
        {sql && (
          <div className="overflow-hidden rounded-lg border border-border bg-card">
            <SqlSnippet sql={sql} />
          </div>
        )}
        {approval && <ApprovalButtons onDecide={(decision) => onDecide(approval.id, decision)} />}
        {failed && tool?.output && <p className="whitespace-pre-wrap text-xs text-destructive-foreground">{tool.output}</p>}
        {canShowRows && (
          <div>
            <button
              type="button"
              aria-expanded={rowsOpen}
              onClick={() => setRowsOpen(!rowsOpen)}
              className="flex h-6 cursor-pointer items-center gap-1 rounded-md px-1.5 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              {rowsOpen ? t("assistant.steps.hideRows") : t("assistant.steps.showRows")}
              {rowsOpen ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
            </button>
            {rowsOpen && rows && shown.length > 0 && (
              <div className="mt-1 overflow-hidden rounded-lg border border-border bg-card">
                <ResultTable
                  columns={rows.columns.map((column) => column.name)}
                  rows={shown.map((row) => row.map((value) => (value === null ? <span className="text-muted-foreground">NULL</span> : typeof value === "string" ? value : JSON.stringify(value))))}
                />
                {rows.rows.length > PREVIEW_ROWS && !showAll && (
                  <button type="button" className="w-full cursor-pointer border-t border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent" onClick={() => setShowAll(true)}>
                    {t("assistant.steps.showAllRows", { count: rows.rows.length })}
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </TraceCollapsibleContent>
    </TraceCollapsible>
  );
}
