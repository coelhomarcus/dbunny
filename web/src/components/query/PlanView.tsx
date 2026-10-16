import { useState } from "react";
import { Check, Copy } from "lucide-react";
import type { ExplainResult } from "@/types";
import { flattenPlan, formatMs, type PlanRow } from "../../lib/plan";
import { cn } from "../../lib/cn";
import Badge from "../ui/Badge";
import Button from "../ui/Button";

/** Share of the execution time a node took by itself, as a colour. */
function heat(share: number) {
  if (share >= 0.5) return "bg-destructive/70";
  if (share >= 0.2) return "bg-warning/70";
  return "bg-primary/45";
}

function Row({ row, executionMs }: { row: PlanRow; executionMs: number | null }) {
  const share = row.selfMs !== null && executionMs ? row.selfMs / executionMs : null;
  return (
    <tr className="border-b border-border align-top hover:bg-accent/50">
      <td className="py-2 pr-4" style={{ paddingLeft: 16 + row.depth * 20 }}>
        <div className="flex items-baseline gap-1.5">
          {row.depth > 0 && <span className="select-none text-muted-foreground/50">└</span>}
          <span className="font-medium">{row.title}</span>
          {row.target && <span className="font-mono text-xs text-muted-foreground">{row.target}</span>}
          {row.misestimate && (
            <Badge variant="warning" className="ml-1">
              rows off ×{row.misestimate.toLocaleString()}
            </Badge>
          )}
        </div>
        {row.details.map((line) => (
          <div key={line} className={cn("font-mono text-xs text-muted-foreground break-words", row.depth > 0 && "pl-4")}>
            {line}
          </div>
        ))}
      </td>
      <td className="whitespace-nowrap px-4 py-2 text-right tabular-nums">
        {row.actualRows !== null ? (
          <>
            {row.actualRows.toLocaleString()}
            <div className="text-xs text-muted-foreground">est. {Math.round(row.estimatedRows).toLocaleString()}</div>
          </>
        ) : (
          <span className="text-muted-foreground">{Math.round(row.estimatedRows).toLocaleString()}</span>
        )}
      </td>
      {executionMs !== null && (
        <td className="w-44 whitespace-nowrap px-4 py-2 tabular-nums">
          <div className="flex items-baseline justify-between gap-2">
            <span>{row.selfMs !== null ? formatMs(row.selfMs) : "—"}</span>
            {share !== null && <span className="text-xs text-muted-foreground">{Math.round(share * 100)}%</span>}
          </div>
          {share !== null && (
            <div className="mt-1 h-1 rounded-full bg-muted">
              <div className={cn("h-1 rounded-full", heat(share))} style={{ width: `${Math.max(2, share * 100)}%` }} />
            </div>
          )}
        </td>
      )}
      <td className="whitespace-nowrap px-4 py-2 text-right text-xs tabular-nums text-muted-foreground">
        {row.node["Total Cost"].toLocaleString(undefined, { maximumFractionDigits: 2 })}
      </td>
    </tr>
  );
}

/** EXPLAIN output as a tree, with the slowest nodes highlighted when it was analyzed. */
export default function PlanView({ explain }: { explain: ExplainResult }) {
  const [copied, setCopied] = useState(false);
  const top = explain.plan[0];
  const rows = flattenPlan(top.Plan);
  const executionMs = explain.analyzed ? (top["Execution Time"] ?? null) : null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-4 py-2 text-[13px]">
        {explain.analyzed ? (
          <>
            <Badge variant="info">Executed, then rolled back</Badge>
            {top["Planning Time"] !== undefined && (
              <span>
                <span className="text-muted-foreground">Planning</span> {formatMs(top["Planning Time"])}
              </span>
            )}
            {executionMs !== null && (
              <span>
                <span className="text-muted-foreground">Execution</span> {formatMs(executionMs)}
              </span>
            )}
          </>
        ) : (
          <span className="text-muted-foreground">Estimated plan. Use Explain analyze to run it and see real times.</span>
        )}
        <Button
          variant="ghost-muted"
          size="xs"
          className="ml-auto"
          onClick={async () => {
            await navigator.clipboard.writeText(JSON.stringify(explain.plan, null, 2));
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? <Check /> : <Copy />} {copied ? "Copied" : "Copy JSON"}
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full text-sm">
          <thead className="sticky top-0 z-[1] select-none">
            <tr className="text-[13px] text-muted-foreground [&>th]:bg-card-header [&>th]:border-b [&>th]:border-border [&>th]:py-2.5 [&>th]:font-medium">
              <th className="px-4 text-left">Step</th>
              <th className="px-4 text-right">{explain.analyzed ? "Rows" : "Est. rows"}</th>
              {executionMs !== null && <th className="px-4 text-left">Own time</th>}
              <th className="px-4 text-right">Cost</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <Row key={i} row={row} executionMs={executionMs} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
