import { useState } from "react";
import { Check, Copy } from "lucide-react";
import type { ExplainResult } from "@/types";
import { flattenPlan, formatMs, mysqlPlanRows, sqlitePlanRows, type DisplayRow } from "../../lib/plan";
import { cn } from "../../lib/cn";
import Badge from "../ui/Badge";
import Button from "../ui/Button";
import { writeClipboardText } from "../../lib/clipboard";
import { useT, fmt } from "../../i18n";

/** A row estimate: whole rows, except that a fraction of a row (MySQL estimates 0.667) isn't rounded to none. */
function formatEstimate(rows: number): string {
  return fmt().number(rows < 10 && rows % 1 !== 0 ? rows : Math.round(rows), { maximumFractionDigits: 3 });
}

/** Share of the execution time a node took by itself, as a colour. */
function heat(share: number) {
  if (share >= 0.5) return "bg-destructive/70";
  if (share >= 0.2) return "bg-warning/70";
  return "bg-primary/45";
}

function Row({ row, executionMs }: { row: DisplayRow; executionMs: number | null }) {
  const t = useT();
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
              {t("query.planView.rowsOff", { factor: fmt().number(row.misestimate) })}
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
            {fmt().number(row.actualRows)}
            {row.estimatedRows !== null && <div className="text-xs text-muted-foreground">{t("query.planView.estimated", { value: formatEstimate(row.estimatedRows) })}</div>}
          </>
        ) : (
          <span className="text-muted-foreground">{row.estimatedRows === null ? "—" : formatEstimate(row.estimatedRows)}</span>
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
        {row.cost === null ? "—" : fmt().number(row.cost, { maximumFractionDigits: 2 })}
      </td>
    </tr>
  );
}

/** SQLite's EXPLAIN QUERY PLAN: one line per step, nested like the plan, without costs or times. */
function SqlitePlanView({ plan }: { plan: Extract<ExplainResult, { format: "sqliteQueryPlan" }>["plan"] }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const lines = sqlitePlanRows(plan);
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-4 py-2 text-[13px]">
        <span className="text-muted-foreground">{t("query.planView.howSqliteWillRunIt")}</span>
        <Button
          variant="ghost-muted"
          size="xs"
          className="ml-auto"
          onClick={async () => {
            await writeClipboardText(JSON.stringify(plan, null, 2));
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? <Check /> : <Copy />} {copied ? t("common.copied") : t("query.planView.copyJson")}
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto py-1">
        {lines.length === 0 ? (
          <p className="px-4 py-3 text-sm text-muted-foreground">{t("query.planView.thisStatementHasNoPlan")}</p>
        ) : (
          lines.map((line, i) => (
            <div key={i} className="border-b border-border py-2 pr-4 text-sm hover:bg-accent/50" style={{ paddingLeft: 16 + line.depth * 20 }}>
              <div className="flex items-baseline gap-1.5">
                {line.depth > 0 && <span className="select-none text-muted-foreground/50">└</span>}
                <span className="font-mono text-[13px]">{line.detail}</span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

/** EXPLAIN output, in the shape the database gave it. */
export default function PlanView({ explain }: { explain: ExplainResult }) {
  switch (explain.format) {
    case "sqliteQueryPlan":
      return <SqlitePlanView plan={explain.plan} />;
    case "mysqlTree":
      return <MysqlPlanView explain={explain} />;
    default:
      return <PostgresPlanView explain={explain} />;
  }
}

/** MySQL's EXPLAIN as a tree: costs and row estimates, and the real times and rows when it was analyzed. */
function MysqlPlanView({ explain }: { explain: Extract<ExplainResult, { format: "mysqlTree" }> }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const rows = mysqlPlanRows(explain.plan);
  const executionMs = explain.analyzed ? (rows[0]?.totalMs ?? null) : null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-4 py-2 text-[13px]">
        {explain.analyzed ? (
          <>
            <Badge variant="info">{t("query.planView.executed")}</Badge>
            {executionMs !== null && (
              <span>
                <span className="text-muted-foreground">{t("query.planView.execution")}</span> {formatMs(executionMs)}
              </span>
            )}
          </>
        ) : (
          <span className="text-muted-foreground">{t("query.planView.estimatedPlanUseExplainAnalyze")}</span>
        )}
        <Button
          variant="ghost-muted"
          size="xs"
          className="ml-auto"
          onClick={async () => {
            await writeClipboardText(JSON.stringify(explain.plan, null, 2));
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? <Check /> : <Copy />} {copied ? t("common.copied") : t("query.planView.copyJson")}
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <p className="px-4 py-3 text-sm text-muted-foreground">{t("query.planView.thisStatementHasNoPlan")}</p>
        ) : (
          <table className="w-full text-sm">
            <thead className="sticky top-0 z-[1] select-none">
              <tr className="text-[13px] text-muted-foreground [&>th]:bg-card-header [&>th]:border-b [&>th]:border-border [&>th]:py-2.5 [&>th]:font-medium">
                <th className="px-4 text-left">{t("query.planView.step")}</th>
                <th className="px-4 text-right">{explain.analyzed ? t("query.planView.rows") : t("query.planView.estRows")}</th>
                {executionMs !== null && <th className="px-4 text-left">{t("query.planView.ownTime")}</th>}
                <th className="px-4 text-right">{t("query.planView.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row, i) => (
                <Row key={i} row={row} executionMs={executionMs} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

/** PostgreSQL's EXPLAIN as a tree, with the slowest nodes highlighted when it was analyzed. */
function PostgresPlanView({ explain }: { explain: Extract<ExplainResult, { format: "postgresJson" }> }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const top = explain.plan[0];
  const rows = flattenPlan(top.Plan);
  const executionMs = explain.analyzed ? (top["Execution Time"] ?? null) : null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-4 py-2 text-[13px]">
        {explain.analyzed ? (
          <>
            <Badge variant="info">{t("query.planView.executedThenRolledBack")}</Badge>
            {top["Planning Time"] !== undefined && (
              <span>
                <span className="text-muted-foreground">{t("query.planView.planning")}</span> {formatMs(top["Planning Time"])}
              </span>
            )}
            {executionMs !== null && (
              <span>
                <span className="text-muted-foreground">{t("query.planView.execution")}</span> {formatMs(executionMs)}
              </span>
            )}
          </>
        ) : (
          <span className="text-muted-foreground">{t("query.planView.estimatedPlanUseExplainAnalyze")}</span>
        )}
        <Button
          variant="ghost-muted"
          size="xs"
          className="ml-auto"
          onClick={async () => {
            await writeClipboardText(JSON.stringify(explain.plan, null, 2));
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? <Check /> : <Copy />} {copied ? t("common.copied") : t("query.planView.copyJson")}
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full text-sm">
          <thead className="sticky top-0 z-[1] select-none">
            <tr className="text-[13px] text-muted-foreground [&>th]:bg-card-header [&>th]:border-b [&>th]:border-border [&>th]:py-2.5 [&>th]:font-medium">
              <th className="px-4 text-left">{t("query.planView.step")}</th>
              <th className="px-4 text-right">{explain.analyzed ? t("query.planView.rows") : t("query.planView.estRows")}</th>
              {executionMs !== null && <th className="px-4 text-left">{t("query.planView.ownTime")}</th>}
              <th className="px-4 text-right">{t("query.planView.cost")}</th>
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
