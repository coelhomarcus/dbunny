import { useEffect, useState, type ReactNode } from "react";
import { useDocuments } from "../contexts/DocumentsContext";
import { ArrowUpRight, Ban, Check, Copy, KeyRound, Link2, MessageSquare, MoreHorizontal, PenLine, Plus, Search, Sparkles, Trash2, TriangleAlert, Asterisk, Equal, Type, X } from "lucide-react";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu";
import StructureChangeDialog, { type ChangeDraft } from "../components/structure/StructureChangeDialog";
import PermissionsEditor from "../components/structure/PermissionsEditor";
import Tooltip from "../components/Tooltip";
import type { ColumnInfo, StructureChange, TableStructure as Structure } from "@/types";
import { TableSkeleton } from "../components/Skeleton";
import Badge from "../components/ui/Badge";
import Button from "../components/ui/Button";
import Checkbox from "../components/Checkbox";
import Input from "../components/ui/Input";
import SqlViewer from "../components/SqlViewer";
import { useApi, useConnection } from "../contexts/useConnection";
import { foreignKeyTarget, shortType } from "../lib/columnMeta";
import { cn } from "../lib/cn";
import { formatBytes } from "../lib/format";
import { writeClipboardText } from "../lib/clipboard";
import { needsRebuild, notAvailableFor } from "../lib/engines";
import { useCapabilities } from "../hooks/useCapabilities";
import NoticeToast from "../components/ui/NoticeToast";

type Section = "columns" | "indexes" | "constraints" | "keys" | "triggers" | "permissions" | "ddl";


const th = "sticky top-0 z-[1] bg-card px-3 pb-1 text-left text-[11px] font-medium text-muted-foreground whitespace-nowrap";
const td = "bg-foreground/[0.045] px-3 py-2 align-middle first:rounded-l-md last:rounded-r-md";

function Grid({ headers, children, empty }: { headers: { label: string; className?: string }[]; children: ReactNode[]; empty: string }) {
  if (children.length === 0) {
    return <p className="px-4 py-10 text-center text-sm text-muted-foreground">{empty}</p>;
  }
  return (
    <table className="w-full border-separate border-spacing-y-1 px-3 text-[13px]">
      <thead>
        <tr>
          {headers.map((h) => (
            <th key={h.label || "actions"} className={cn(th, h.className)}>
              {h.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>{children}</tbody>
    </table>
  );
}

function heads(labels: string[], centered: string[] = []): { label: string; className?: string }[] {
  return labels.map((label) => ({ label, className: centered.includes(label) ? "text-center" : undefined }));
}

function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return <code className={cn("font-mono text-xs break-words", className)}>{children}</code>;
}

/** A link to another table that keeps the row's text style. */
function TableLink({ schema, table, children }: { schema: string; table: string; children: ReactNode }) {
  const { openDocument } = useDocuments();
  return (
    <button
      type="button"
      onClick={() => openDocument({ kind: "data", schema, name: table, icon: "table" })}
      className="group inline-flex items-center gap-1 rounded text-left font-mono text-xs text-info-foreground hover:underline cursor-pointer"
    >
      {children}
      <ArrowUpRight size={12} className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
    </button>
  );
}

/** Toolbar above a section's table, for its "add" action. */
function SectionBar({ children }: { children: ReactNode }) {
  return <div className="flex items-center justify-end gap-2 px-3 pt-3 pb-1">{children}</div>;
}

/** A section's "add" action; disabled, saying why, when the engine can't do it. */
function SectionAction({ unavailable, onClick, children }: { unavailable?: string; onClick: () => void; children: ReactNode }) {
  if (!unavailable) {
    return (
      <Button variant="ghost-muted" size="xs" onClick={onClick}>
        {children}
      </Button>
    );
  }
  return (
    <Tooltip label={unavailable} side="bottom">
      <Button variant="ghost-muted" size="xs" disabled>
        {children}
      </Button>
    </Tooltip>
  );
}

function ColumnsSection({
  columns,
  comments,
  nullableEditable,
  onToggleNullable,
  onDrop,
  onColumnMenu,
}: {
  columns: ColumnInfo[];
  /** The engine keeps comments on columns. */
  comments: boolean;
  /** Nullable can be switched in place. Primary stays a marker: changing it rebuilds the key. */
  nullableEditable: boolean;
  onToggleNullable?: (column: ColumnInfo) => void;
  onDrop?: (column: ColumnInfo) => void;
  onColumnMenu?: (column: ColumnInfo, x: number, y: number) => void;
}) {
  return (
    <Grid
      headers={heads(["Name", "Type", "Nullable", "Default value", ...(comments ? ["Comment"] : []), "Primary", ...((onDrop || onColumnMenu) ? [""] : [])], ["Nullable", "Primary"])}
      empty="No columns"
    >
      {columns.map((col) => {
        const target = foreignKeyTarget(col);
        const nullableLocked = !nullableEditable || col.isPrimaryKey;
        return (
          <tr
            key={col.name}
            className="group"
            onContextMenu={
              onColumnMenu
                ? (e) => {
                    e.preventDefault();
                    onColumnMenu(col, e.clientX, e.clientY);
                  }
                : undefined
            }
          >
            <td className={td}>
              <span className="flex items-center gap-1.5 font-medium">
                {!col.isPrimaryKey && col.foreignKey && <Link2 size={12} className="shrink-0 text-info-foreground" aria-label="Foreign key" />}
                {col.name}
              </span>
              {col.foreignKey && target && (
                <span className="mt-0.5 flex items-center gap-1 text-xs text-muted-foreground">
                  <TableLink schema={col.foreignKey.schema} table={col.foreignKey.table}>
                    {target}
                  </TableLink>
                </span>
              )}
            </td>
            <td className={td}>
              <Mono>{shortType(col.typeName)}</Mono>
              {col.enumValues && <div className="mt-0.5 text-xs text-muted-foreground">{col.enumValues.join(" · ")}</div>}
            </td>
            <td className={cn(td, "w-20 text-center")}>
              <span className="inline-flex">
                <Checkbox
                  checked={col.isNullable}
                  disabled={nullableLocked || !onToggleNullable}
                  onChange={() => onToggleNullable?.(col)}
                />
              </span>
            </td>
            <td className={td}>
              {col.defaultValue !== null ? (
                <Mono className="text-muted-foreground">{col.defaultValue}</Mono>
              ) : col.isGenerated ? (
                <Badge>
                  <Sparkles /> generated
                </Badge>
              ) : (
                <span className="text-muted-foreground/70">(NULL)</span>
              )}
            </td>
            {comments && (
              <td className={cn(td, "max-w-80 text-muted-foreground")}>{col.comment?.trim() ? col.comment : <span className="text-muted-foreground/70">(NULL)</span>}</td>
            )}
            <td className={cn(td, "w-20 text-center")}>
              <span className="inline-flex">
                <Checkbox checked={col.isPrimaryKey} disabled onChange={() => {}} />
              </span>
              {col.isPrimaryKey && <KeyRound size={12} className="sr-only" aria-label="Primary key" />}
            </td>
            {(onDrop || onColumnMenu) && (
              <td className={cn(td, "w-px whitespace-nowrap")}>
                <span className="flex items-center justify-end gap-0.5">
                  {onColumnMenu && (
                    <Button
                      variant="ghost-muted"
                      size="icon-xs"
                      aria-label={`Change ${col.name}`}
                      className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                      onClick={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        onColumnMenu(col, r.left, r.bottom + 4);
                      }}
                    >
                      <MoreHorizontal />
                    </Button>
                  )}
                  {onDrop && (
                    <Button variant="ghost-muted" size="icon-xs" aria-label={`Drop ${col.name}`} onClick={() => onDrop(col)}>
                      <X />
                    </Button>
                  )}
                </span>
              </td>
            )}
          </tr>
        );
      })}
    </Grid>
  );
}

function IndexesSection({ structure, onDrop, onMenu }: { structure: Structure; onDrop?: (index: string) => void; onMenu: (items: ContextMenuItem[], x: number, y: number) => void }) {
  return (
    <Grid headers={heads(["Index", "Columns", "Type", "Size", ...(onDrop ? [""] : [])])} empty="No indexes">
      {structure.indexes.map((index) => (
        <tr
          key={index.name}
          onContextMenu={(e) => {
            e.preventDefault();
            const items: ContextMenuItem[] = [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(index.name) }];
            if (onDrop && !index.backsConstraint) items.push({ label: "Drop…", icon: <Trash2 />, destructive: true, onSelect: () => onDrop(index.name) });
            onMenu(items, e.clientX, e.clientY);
          }}
        >
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
          {onDrop && (
            <td className={cn(td, "w-px py-1.5")}>
              {/* Constraint indexes go away with their constraint */}
              {!index.backsConstraint && (
                <Button variant="destructive-ghost" size="icon-xs" aria-label={`Drop index ${index.name}`} onClick={() => onDrop(index.name)}>
                  <Trash2 />
                </Button>
              )}
            </td>
          )}
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

/** A row's drop button, in the last column of an editable section; disabled, saying why, when the engine can't do it. */
function DropCell({ label, unavailable, onClick }: { label: string; unavailable?: string; onClick: () => void }) {
  return (
    <td className={cn(td, "w-px py-1.5")}>
      <Tooltip label={unavailable || label} side="left">
        <Button variant="destructive-ghost" size="icon-xs" aria-label={label} onClick={onClick} disabled={!!unavailable}>
          <Trash2 />
        </Button>
      </Tooltip>
    </td>
  );
}

function ConstraintsSection({
  structure,
  unavailable,
  onDrop,
  onMenu,
}: {
  structure: Structure;
  unavailable?: string;
  onDrop?: (constraint: string) => void;
  onMenu: (items: ContextMenuItem[], x: number, y: number) => void;
}) {
  return (
    <Grid headers={heads(["Constraint", "Kind", "Definition", ...(onDrop ? [""] : [])])} empty="No constraints">
      {structure.constraints.map((c) => (
        <tr
          key={c.name}
          onContextMenu={(e) => {
            e.preventDefault();
            const items: ContextMenuItem[] = [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(c.name) }];
            if (onDrop) items.push({ label: "Drop…", icon: <Trash2 />, destructive: true, disabled: !!unavailable, onSelect: () => onDrop(c.name) });
            onMenu(items, e.clientX, e.clientY);
          }}
        >
          <td className={td}>
            <Mono className="text-foreground">{c.name}</Mono>
          </td>
          <td className={td}>
            <Badge variant={CONSTRAINT_BADGE[c.kind] ?? "neutral"}>{c.kind}</Badge>
          </td>
          <td className={td}>
            <Mono className="text-muted-foreground">{c.definition}</Mono>
          </td>
          {onDrop && <DropCell label={`Drop constraint ${c.name}`} unavailable={unavailable} onClick={() => onDrop(c.name)} />}
        </tr>
      ))}
    </Grid>
  );
}

function KeysSection({
  structure,
  unavailable,
  onDrop,
  onMenu,
}: {
  structure: Structure;
  unavailable?: string;
  onDrop?: (constraint: string) => void;
  onMenu: (items: ContextMenuItem[], x: number, y: number) => void;
}) {
  return (
    <div className="pb-6">
      <Grid headers={heads(["Foreign key", "Columns", "References", "On update", "On delete", ...(onDrop ? [""] : [])])} empty="This table has no foreign keys">
        {structure.foreignKeys.map((fk) => (
          <tr
            key={fk.name}
            onContextMenu={(e) => {
              e.preventDefault();
              const items: ContextMenuItem[] = [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(fk.name) }];
              if (onDrop) items.push({ label: "Drop…", icon: <Trash2 />, destructive: true, disabled: !!unavailable, onSelect: () => onDrop(fk.name) });
              onMenu(items, e.clientX, e.clientY);
            }}
          >
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
            {onDrop && <DropCell label={`Drop foreign key ${fk.name}`} unavailable={unavailable} onClick={() => onDrop(fk.name)} />}
          </tr>
        ))}
      </Grid>

      <h3 className="px-4 pt-6 pb-2 text-[13px] font-medium text-muted-foreground">Referenced by</h3>
      <Grid headers={heads(["Table", "Columns", "Constraint"])} empty="No other table points at this one">
        {structure.referencedBy.map((ref) => (
          <tr
            key={`${ref.schema}.${ref.table}.${ref.name}`}
            onContextMenu={(e) => {
              e.preventDefault();
              onMenu([{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(ref.name) }], e.clientX, e.clientY);
            }}
          >
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

function TriggersSection({
  structure,
  unavailable,
  onToggle,
  onMenu,
}: {
  structure: Structure;
  /** Why the toggle is disabled, when the engine can't switch triggers. */
  unavailable?: string;
  onToggle?: (trigger: string, enabled: boolean) => void;
  onMenu: (items: ContextMenuItem[], x: number, y: number) => void;
}) {
  return (
    <Grid headers={heads(["Trigger", "When", "For each", "Function", "Status"])} empty="No triggers">
      {structure.triggers.map((t) => (
        <tr
          key={t.name}
          onContextMenu={(e) => {
            e.preventDefault();
            const items: ContextMenuItem[] = [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(t.name) }];
            if (onToggle) {
              items.push({
                label: t.enabled ? "Disable" : "Enable",
                disabled: !!unavailable,
                onSelect: () => onToggle(t.name, !t.enabled),
              });
            }
            onMenu(items, e.clientX, e.clientY);
          }}
        >
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
            {onToggle && unavailable ? (
              <Tooltip label={unavailable} side="left">
                <button type="button" disabled className="rounded-md disabled:pointer-events-none disabled:opacity-55">
                  <Badge variant={t.enabled ? "success" : "neutral"}>{t.enabled ? "enabled" : "disabled"}</Badge>
                </button>
              </Tooltip>
            ) : onToggle ? (
              <Tooltip label={t.enabled ? "Disable: it stops firing until enabled again" : "Enable: it fires again"} side="left">
                <button type="button" onClick={() => onToggle(t.name, !t.enabled)} className="cursor-pointer rounded-md transition-opacity hover:opacity-80">
                  <Badge variant={t.enabled ? "success" : "neutral"}>{t.enabled ? "enabled" : "disabled"}</Badge>
                </button>
              </Tooltip>
            ) : (
              <Badge variant={t.enabled ? "success" : "neutral"}>{t.enabled ? "enabled" : "disabled"}</Badge>
            )}
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
            await writeClipboardText(ddl);
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

export default function TableStructure({ schema, name: table }: { schema: string; name: string }) {
  const api = useApi();
  const key = `${schema}.${table}`;
  const [result, setResult] = useState<{ key: string; structure?: Structure; error?: string } | null>(null);
  const [section, setSection] = useState<Section>("columns");
  const { connectionInfo } = useConnection();
  const readOnly = connectionInfo?.readOnly ?? false;
  const capabilities = useCapabilities();
  /** Tooltip of the actions the engine can't do. */
  const unavailable = connectionInfo ? notAvailableFor(connectionInfo.engine) : "";
  // Constraints change by rebuilding the table in SQLite, which isn't done yet
  const constraintsUnavailable = connectionInfo ? needsRebuild(connectionInfo.engine) : "";
  // Bumped after a change to load the structure again
  const [version, setVersion] = useState(0);
  const [draft, setDraft] = useState<ChangeDraft | null>(null);
  const [columnMenu, setColumnMenu] = useState<{ column: ColumnInfo; x: number; y: number } | null>(null);
  const [listMenu, setListMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const [columnFilter, setColumnFilter] = useState("");
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    setColumnFilter("");
  }, [schema, table]);

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
  }, [schema, table, api, version]);

  const loading = !result || result.key !== key;
  const structure = loading ? undefined : result.structure;
  const meta = structure?.meta;
  const isView = meta?.kind === "view" || meta?.kind === "materialized view";
  const canEdit = !readOnly && !isView && structure !== undefined;

  /** Runs a one-click change (no form to fill) and reloads. */
  function applyNow(change: StructureChange) {
    api
      .applyStructureChange(schema!, table!, change)
      .then(() => {
        setNotice(null);
        setVersion((v) => v + 1);
      })
      .catch((err) => setNotice(typeof err === "string" ? err : "The change failed"));
  }

  function columnMenuItems(column: ColumnInfo): ContextMenuItem[] {
    return [
      { label: "Rename…", icon: <PenLine />, onSelect: () => setDraft({ kind: "renameColumn", column }) },
      ...(capabilities.alterColumn
        ? ([
            { label: "Change type…", icon: <Type />, onSelect: () => setDraft({ kind: "setType", column }) },
            {
              label: column.isNullable ? "Make required (NOT NULL)" : "Allow NULL",
              icon: column.isNullable ? <Asterisk /> : <Ban />,
              disabled: column.isPrimaryKey,
              onSelect: () => applyNow({ kind: "setNullable", name: column.name, nullable: !column.isNullable }),
            },
            { label: "Default…", icon: <Equal />, disabled: column.isGenerated, onSelect: () => setDraft({ kind: "setDefault", column }) },
          ] as ContextMenuItem[])
        : []),
      ...(capabilities.comments
        ? ([{ label: "Comment…", icon: <MessageSquare />, onSelect: () => setDraft({ kind: "setComment", column }) }] as ContextMenuItem[])
        : []),
      "separator",
      { label: "Drop column…", icon: <Trash2 />, destructive: true, onSelect: () => setDraft({ kind: "dropColumn", column }) },
    ];
  }

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
              ...(capabilities.permissions ? [{ value: "permissions", label: <>Permissions {count(structure.privileges.length)}</> }] : []),
            ] as { value: Section; label: ReactNode }[])),
        { value: "ddl", label: "DDL" },
      ]
    : [];
  // A view has fewer sections; fall back to the columns if the chosen one isn't there
  const shown = sections.some((s) => s.value === section) ? section : "columns";

  const columnQuery = columnFilter.trim().toLowerCase();
  const visibleColumns = structure?.columns.filter((col) => !columnQuery || col.name.toLowerCase().includes(columnQuery)) ?? [];

  return (
    <div className="h-full flex flex-col">
      {loading ? (
        <TableSkeleton columns={5} rows={10} />
      ) : !structure ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-destructive-foreground">{result.error}</div>
      ) : (
        <>
          <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 px-4 pt-3 pb-2 text-[13px]">
            <Fact label="Kind">{meta!.kind}</Fact>
            <Fact label="Owner">{meta!.owner}</Fact>
            {meta!.rowEstimate !== null && <Fact label="Rows">~{meta!.rowEstimate.toLocaleString()}</Fact>}
            {meta!.totalBytes !== null && (
              <Fact label="Size">
                {formatBytes(meta!.totalBytes)}
                {meta!.indexesBytes ? <span className="text-muted-foreground"> ({formatBytes(meta!.indexesBytes)} indexes)</span> : null}
              </Fact>
            )}
            {meta!.comment && <span className="min-w-0 truncate text-muted-foreground">{meta!.comment}</span>}
          </div>

          <div className="flex h-10 shrink-0 items-end justify-center border-b border-border px-2">
            <div className="flex min-w-0 max-w-full items-end gap-0.5 overflow-x-auto">
              {sections.map((item) => {
                const active = item.value === shown;
                return (
                  <button
                    key={item.value}
                    type="button"
                    onClick={() => setSection(item.value)}
                    className={cn(
                      "-mb-px shrink-0 cursor-pointer border-b-2 px-3 pb-2 text-[13px] font-medium",
                      active ? "border-foreground text-foreground" : "border-transparent text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {item.label}
                  </button>
                );
              })}
            </div>
          </div>

          {structure.warnings.length > 0 && (
            <div className="flex shrink-0 items-start gap-2 border-b border-border bg-warning/8 px-4 py-2 text-[13px] text-warning-foreground">
              <TriangleAlert size={14} className="mt-0.5 shrink-0" />
              <div>{structure.warnings.map((w) => <div key={w}>{w}</div>)}</div>
            </div>
          )}

          <div className={cn("min-h-0 flex-1", shown === "ddl" || shown === "permissions" ? "overflow-hidden" : "overflow-auto")}>
            {shown === "columns" && (
              <>
                <div className="flex items-center gap-2 px-3 pt-3 pb-1">
                  <div className="relative min-w-0 max-w-sm flex-1">
                    <Search size={14} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground" />
                    <Input
                      value={columnFilter}
                      onChange={(e) => setColumnFilter(e.target.value)}
                      placeholder="Filter columns"
                      aria-label="Filter columns"
                      spellCheck={false}
                      className="h-8 border-0 bg-foreground/[0.04] pl-8 shadow-none dark:bg-foreground/[0.04]"
                    />
                  </div>
                  {canEdit && (
                    <Button variant="primary" size="sm" className="ml-auto" onClick={() => setDraft({ kind: "addColumn" })}>
                      <Plus /> Column
                    </Button>
                  )}
                </div>
                {structure.columns.length > 0 && visibleColumns.length === 0 ? (
                  <p className="px-4 py-6 text-center text-sm text-muted-foreground">No columns match “{columnFilter.trim()}”.</p>
                ) : (
                  <ColumnsSection
                    columns={visibleColumns}
                    comments={capabilities.comments}
                    nullableEditable={capabilities.alterColumn}
                    onToggleNullable={
                      canEdit && capabilities.alterColumn
                        ? (column) => applyNow({ kind: "setNullable", name: column.name, nullable: !column.isNullable })
                        : undefined
                    }
                    onDrop={canEdit ? (column) => setDraft({ kind: "dropColumn", column }) : undefined}
                    onColumnMenu={canEdit ? (column, x, y) => setColumnMenu({ column, x, y }) : undefined}
                  />
                )}
              </>
            )}
            {shown === "indexes" && (
              <>
                {canEdit && (
                  <SectionBar>
                    <Button variant="ghost-muted" size="xs" onClick={() => setDraft({ kind: "createIndex" })}>
                      <Plus /> Create index
                    </Button>
                  </SectionBar>
                )}
                <IndexesSection
                  structure={structure}
                  onDrop={canEdit ? (index) => setDraft({ kind: "dropIndex", index }) : undefined}
                  onMenu={(items, x, y) => setListMenu({ items, x, y })}
                />
              </>
            )}
            {shown === "constraints" && (
              <>
                {canEdit && (
                  <SectionBar>
                    <SectionAction unavailable={capabilities.constraints ? undefined : constraintsUnavailable} onClick={() => setDraft({ kind: "addUnique" })}>
                      <Plus /> Add unique
                    </SectionAction>
                    <SectionAction unavailable={capabilities.constraints ? undefined : constraintsUnavailable} onClick={() => setDraft({ kind: "addCheck" })}>
                      <Plus /> Add check
                    </SectionAction>
                  </SectionBar>
                )}
                <ConstraintsSection
                  structure={structure}
                  unavailable={capabilities.constraints ? undefined : constraintsUnavailable}
                  onDrop={canEdit ? (constraint) => setDraft({ kind: "dropConstraint", constraint }) : undefined}
                  onMenu={(items, x, y) => setListMenu({ items, x, y })}
                />
              </>
            )}
            {shown === "keys" && (
              <>
                {canEdit && (
                  <SectionBar>
                    <SectionAction unavailable={capabilities.constraints ? undefined : constraintsUnavailable} onClick={() => setDraft({ kind: "addForeignKey" })}>
                      <Plus /> Add foreign key
                    </SectionAction>
                  </SectionBar>
                )}
                <KeysSection
                  structure={structure}
                  unavailable={capabilities.constraints ? undefined : constraintsUnavailable}
                  onDrop={canEdit ? (constraint) => setDraft({ kind: "dropConstraint", constraint }) : undefined}
                  onMenu={(items, x, y) => setListMenu({ items, x, y })}
                />
              </>
            )}
            {shown === "triggers" && (
              <TriggersSection
                structure={structure}
                unavailable={capabilities.triggerToggle ? undefined : unavailable}
                onToggle={canEdit ? (name, enabled) => applyNow({ kind: "setTriggerEnabled", name, enabled }) : undefined}
                onMenu={(items, x, y) => setListMenu({ items, x, y })}
              />
            )}
            {shown === "permissions" && (
              <PermissionsEditor
                // Fresh permissions from the server start the editor over
                key={JSON.stringify(structure.privileges)}
                schema={schema!}
                table={table!}
                owner={meta!.owner}
                privileges={structure.privileges}
                canEdit={canEdit}
                onDone={(error) => {
                  setNotice(error);
                  setVersion((v) => v + 1);
                }}
              />
            )}
            {shown === "ddl" && <DdlSection ddl={structure.ddl} />}
          </div>
        </>
      )}
      <ContextMenu
        position={columnMenu}
        items={columnMenu ? columnMenuItems(columnMenu.column) : []}
        onClose={() => setColumnMenu(null)}
      />
      <ContextMenu position={listMenu} items={listMenu?.items ?? []} onClose={() => setListMenu(null)} />
      <StructureChangeDialog
        draft={draft}
        schema={schema!}
        table={table!}
        columns={structure?.columns ?? []}
        onDone={() => setVersion((v) => v + 1)}
        onClose={() => setDraft(null)}
      />
      <NoticeToast notice={notice ? { tone: "error", text: notice } : null} onDismiss={() => setNotice(null)} />
    </div>
  );
}
