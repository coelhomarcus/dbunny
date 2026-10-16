import { useEffect, useState, type ReactNode } from "react";
import { NavLink, useNavigate, useParams } from "react-router";
import { ArrowUpRight, Check, Copy, KeyRound, Link2, Sparkles, TriangleAlert } from "lucide-react";
import type { ColumnInfo, TableStructure as Structure } from "@/types";
import { segmentedItemClass, segmentedListClass } from "../components/ui/styles";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { TableSkeleton } from "../components/Skeleton";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import Badge from "../components/ui/Badge";
import Button from "../components/ui/Button";
import SqlViewer from "../components/SqlViewer";
import { useApi } from "../contexts/useConnection";
import { foreignKeyTarget, shortType } from "../lib/columnMeta";
import { cn } from "../lib/cn";

type Section = "columns" | "indexes" | "constraints" | "keys" | "triggers" | "ddl";

/** "1.4 MB" */
function formatBytes(bytes: number): string {
  const units = ["B", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${unit === 0 ? value : value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

function tablePath(schema: string, table: string) {
  return `/db/${encodeURIComponent(schema)}/${encodeURIComponent(table)}`;
}

const th = "sticky top-0 z-[1] bg-card-header border-b border-border px-4 py-2.5 text-left text-[13px] font-medium text-muted-foreground whitespace-nowrap";
const td = "border-b border-border px-4 py-2 align-top";

function Grid({ headers, children, empty }: { headers: string[]; children: ReactNode[]; empty: string }) {
  if (children.length === 0) {
    return <p className="px-4 py-10 text-center text-sm text-muted-foreground">{empty}</p>;
  }
  return (
    <table className="w-full text-sm">
      <thead>
        <tr>
          {headers.map((h) => (
            <th key={h} className={th}>
              {h}
            </th>
          ))}
        </tr>
      </thead>
      <tbody className="[&>tr:nth-child(even)]:bg-muted [&>tr]:bg-card">{children}</tbody>
    </table>
  );
}

function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return <code className={cn("font-mono text-xs break-words", className)}>{children}</code>;
}

/** A link to another table that keeps the row's text style. */
function TableLink({ schema, table, children }: { schema: string; table: string; children: ReactNode }) {
  const navigate = useNavigate();
  return (
    <button
      type="button"
      onClick={() => navigate(tablePath(schema, table))}
      className="group inline-flex items-center gap-1 rounded text-left font-mono text-xs text-info-foreground hover:underline cursor-pointer"
    >
      {children}
      <ArrowUpRight size={12} className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
    </button>
  );
}

function ColumnsSection({ columns }: { columns: ColumnInfo[] }) {
  return (
    <Grid headers={["#", "Column", "Type", "Nullable", "Default", "Comment"]} empty="No columns">
      {columns.map((col) => {
        const target = foreignKeyTarget(col);
        return (
          <tr key={col.name}>
            <td className={cn(td, "w-10 text-muted-foreground/70 tabular-nums")}>{col.ordinalPosition}</td>
            <td className={td}>
              <span className="flex items-center gap-1.5 font-medium">
                {col.isPrimaryKey && <KeyRound size={12} className="shrink-0 text-warning-foreground" aria-label="Primary key" />}
                {!col.isPrimaryKey && col.foreignKey && <Link2 size={12} className="shrink-0 text-info-foreground" aria-label="Foreign key" />}
                {col.name}
              </span>
              {col.foreignKey && target && (
                <span className="mt-0.5 flex items-center gap-1 text-xs text-muted-foreground">
                  →
                  <TableLink schema={col.foreignKey.schema} table={col.foreignKey.table}>
                    {target}
                  </TableLink>
                </span>
              )}
            </td>
            <td className={td}>
              <Mono className="text-warning-foreground">{shortType(col.typeName)}</Mono>
              {col.enumValues && (
                <div className="mt-0.5 text-xs text-muted-foreground">{col.enumValues.join(" · ")}</div>
              )}
            </td>
            <td className={td}>
              {col.isNullable ? <span className="text-muted-foreground">yes</span> : <span>no</span>}
            </td>
            <td className={td}>
              {col.isGenerated ? (
                <Badge>
                  <Sparkles /> generated
                </Badge>
              ) : col.defaultValue !== null ? (
                <Mono className="text-muted-foreground">{col.defaultValue}</Mono>
              ) : (
                <span className="text-muted-foreground/60">—</span>
              )}
            </td>
            <td className={cn(td, "max-w-80 text-muted-foreground")}>{col.comment ?? ""}</td>
          </tr>
        );
      })}
    </Grid>
  );
}

function IndexesSection({ structure }: { structure: Structure }) {
  return (
    <Grid headers={["Index", "Columns", "Type", "Size"]} empty="No indexes">
      {structure.indexes.map((index) => (
        <tr key={index.name}>
          <td className={td}>
            <div className="flex flex-wrap items-center gap-1.5">
              <Mono className="text-foreground">{index.name}</Mono>
              {index.isPrimary ? (
                <Badge variant="warning">primary</Badge>
              ) : index.isUnique ? (
                <Badge variant="info">unique</Badge>
              ) : null}
            </div>
            {index.predicate && <div className="mt-0.5 font-mono text-xs text-muted-foreground">WHERE {index.predicate}</div>}
          </td>
          <td className={td}>
            <div className="flex flex-wrap gap-1">
              {index.columns.map((c, i) => (
                <Badge key={i} className="font-mono font-normal">
                  {c}
                </Badge>
              ))}
            </div>
          </td>
          <td className={cn(td, "text-muted-foreground")}>{index.method}</td>
          <td className={cn(td, "whitespace-nowrap tabular-nums text-muted-foreground")}>
            {index.bytes !== null ? formatBytes(index.bytes) : "—"}
          </td>
        </tr>
      ))}
    </Grid>
  );
}

const CONSTRAINT_BADGE: Record<string, "warning" | "info" | "success" | "neutral"> = {
  "primary key": "warning",
  "foreign key": "info",
  unique: "success",
};

function ConstraintsSection({ structure }: { structure: Structure }) {
  return (
    <Grid headers={["Constraint", "Kind", "Definition"]} empty="No constraints">
      {structure.constraints.map((c) => (
        <tr key={c.name}>
          <td className={td}>
            <Mono className="text-foreground">{c.name}</Mono>
          </td>
          <td className={td}>
            <Badge variant={CONSTRAINT_BADGE[c.kind] ?? "neutral"}>{c.kind}</Badge>
          </td>
          <td className={td}>
            <Mono className="text-muted-foreground">{c.definition}</Mono>
          </td>
        </tr>
      ))}
    </Grid>
  );
}

function KeysSection({ structure }: { structure: Structure }) {
  return (
    <div className="pb-6">
      <Grid headers={["Foreign key", "Columns", "References", "On update", "On delete"]} empty="This table has no foreign keys">
        {structure.foreignKeys.map((fk) => (
          <tr key={fk.name}>
            <td className={td}>
              <Mono className="text-foreground">{fk.name}</Mono>
            </td>
            <td className={td}>
              <Mono>{fk.columns.join(", ")}</Mono>
            </td>
            <td className={td}>
              <TableLink schema={fk.refSchema} table={fk.refTable}>
                {fk.refSchema}.{fk.refTable}({fk.refColumns.join(", ")})
              </TableLink>
            </td>
            <td className={cn(td, "text-xs text-muted-foreground")}>{fk.onUpdate}</td>
            <td className={cn(td, "text-xs", fk.onDelete === "CASCADE" ? "text-destructive-foreground" : "text-muted-foreground")}>
              {fk.onDelete}
            </td>
          </tr>
        ))}
      </Grid>

      <h3 className="px-4 pt-6 pb-2 text-[13px] font-medium text-muted-foreground">Referenced by</h3>
      <Grid headers={["Table", "Columns", "Constraint"]} empty="No other table points at this one">
        {structure.referencedBy.map((ref) => (
          <tr key={`${ref.schema}.${ref.table}.${ref.name}`}>
            <td className={td}>
              <TableLink schema={ref.schema} table={ref.table}>
                {ref.schema}.{ref.table}
              </TableLink>
            </td>
            <td className={td}>
              <Mono>{ref.columns.join(", ")}</Mono>
            </td>
            <td className={td}>
              <Mono className="text-muted-foreground">{ref.name}</Mono>
            </td>
          </tr>
        ))}
      </Grid>
    </div>
  );
}

function TriggersSection({ structure }: { structure: Structure }) {
  return (
    <Grid headers={["Trigger", "When", "For each", "Function", "Status"]} empty="No triggers">
      {structure.triggers.map((t) => (
        <tr key={t.name}>
          <td className={td}>
            <Mono className="text-foreground">{t.name}</Mono>
          </td>
          <td className={cn(td, "text-xs")}>
            {t.timing} {t.events.join(" OR ")}
          </td>
          <td className={cn(td, "text-xs text-muted-foreground")}>{t.level.toLowerCase()}</td>
          <td className={td}>
            <Mono className="text-muted-foreground">{t.function}()</Mono>
          </td>
          <td className={td}>
            <Badge variant={t.enabled ? "success" : "neutral"}>{t.enabled ? "enabled" : "disabled"}</Badge>
          </td>
        </tr>
      ))}
    </Grid>
  );
}

function DdlSection({ ddl }: { ddl: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-end border-b border-border px-3 py-1.5">
        <Button
          variant="ghost-muted"
          size="xs"
          onClick={async () => {
            await navigator.clipboard.writeText(ddl);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? <Check /> : <Copy />} {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <SqlViewer value={ddl} />
      </div>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <span className="flex items-center gap-1.5 whitespace-nowrap">
      <span className="text-muted-foreground">{label}</span>
      <span className="tabular-nums">{children}</span>
    </span>
  );
}

export default function TableStructure() {
  const { schema, table } = useParams<{ schema: string; table: string }>();
  const api = useApi();
  const key = `${schema}.${table}`;
  const [result, setResult] = useState<{ key: string; structure?: Structure; error?: string } | null>(null);
  const [section, setSection] = useState<Section>("columns");

  useEffect(() => {
    if (!schema || !table) return;
    let stale = false;
    const k = `${schema}.${table}`;
    api
      .getTableStructure(schema, table)
      .then((structure) => !stale && setResult({ key: k, structure }))
      .catch((e) => !stale && setResult({ key: k, error: typeof e === "string" ? e : String(e) }));
    return () => {
      stale = true;
    };
  }, [schema, table, api]);

  const loading = !result || result.key !== key;
  const structure = loading ? undefined : result.structure;
  const meta = structure?.meta;
  const isView = meta?.kind === "view" || meta?.kind === "materialized view";

  const count = (n: number) => <span className="text-xs tabular-nums text-muted-foreground">{n}</span>;
  const sections: { value: Section; label: ReactNode }[] = structure
    ? [
        { value: "columns", label: <>Columns {count(structure.columns.length)}</> },
        ...(isView
          ? []
          : ([
              { value: "indexes", label: <>Indexes {count(structure.indexes.length)}</> },
              { value: "constraints", label: <>Constraints {count(structure.constraints.length)}</> },
              { value: "keys", label: <>Foreign keys {count(structure.foreignKeys.length + structure.referencedBy.length)}</> },
              { value: "triggers", label: <>Triggers {count(structure.triggers.length)}</> },
            ] as { value: Section; label: ReactNode }[])),
        { value: "ddl", label: "DDL" },
      ]
    : [];
  // A view has fewer sections; fall back to the columns if the chosen one isn't there
  const shown = sections.some((s) => s.value === section) ? section : "columns";

  return (
    <div className="h-full flex flex-col">
      <PageHeader className="justify-between">
        <HeaderTitle label={`Structure · ${schema}`}>{table}</HeaderTitle>
        <div className={segmentedListClass}>
          <NavLink to={tablePath(schema!, table!)} end className={({ isActive }) => segmentedItemClass(isActive)}>
            Data
          </NavLink>
          <NavLink to={`${tablePath(schema!, table!)}/structure`} className={({ isActive }) => segmentedItemClass(isActive)}>
            Structure
          </NavLink>
        </div>
      </PageHeader>

      {loading ? (
        <TableSkeleton columns={5} rows={10} />
      ) : !structure ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-destructive-foreground">{result.error}</div>
      ) : (
        <>
          <div className="flex shrink-0 flex-wrap items-center gap-x-5 gap-y-2 border-b border-border px-4 py-2.5 text-[13px]">
            <SegmentedTabs value={shown} options={sections} onChange={setSection} />
            <div className="ml-auto flex flex-wrap items-center gap-x-4 gap-y-1">
              <Fact label="Kind">{meta!.kind}</Fact>
              <Fact label="Owner">{meta!.owner}</Fact>
              {meta!.rowEstimate !== null && <Fact label="Rows">~{meta!.rowEstimate.toLocaleString()}</Fact>}
              {meta!.totalBytes !== null && (
                <Fact label="Size">
                  {formatBytes(meta!.totalBytes)}
                  {meta!.indexesBytes ? (
                    <span className="text-muted-foreground"> ({formatBytes(meta!.indexesBytes)} indexes)</span>
                  ) : null}
                </Fact>
              )}
            </div>
          </div>

          {meta!.comment && (
            <p className="shrink-0 border-b border-border px-4 py-2 text-[13px] text-muted-foreground">{meta!.comment}</p>
          )}

          {structure.warnings.length > 0 && (
            <div className="flex shrink-0 items-start gap-2 border-b border-border bg-warning/8 px-4 py-2 text-[13px] text-warning-foreground">
              <TriangleAlert size={14} className="mt-0.5 shrink-0" />
              <div>{structure.warnings.map((w) => <div key={w}>{w}</div>)}</div>
            </div>
          )}

          <div className={cn("min-h-0 flex-1", shown === "ddl" ? "overflow-hidden" : "overflow-auto")}>
            {shown === "columns" && <ColumnsSection columns={structure.columns} />}
            {shown === "indexes" && <IndexesSection structure={structure} />}
            {shown === "constraints" && <ConstraintsSection structure={structure} />}
            {shown === "keys" && <KeysSection structure={structure} />}
            {shown === "triggers" && <TriggersSection structure={structure} />}
            {shown === "ddl" && <DdlSection ddl={structure.ddl} />}
          </div>
        </>
      )}
    </div>
  );
}
