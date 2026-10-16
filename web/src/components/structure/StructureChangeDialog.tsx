import { useEffect, useState } from "react";
import { Columns3, ListTree, PenLine, Trash2 } from "lucide-react";
import type { ColumnInfo, StructureChange } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";

/** Which form to show; fields not chosen yet come from the column being changed. */
export type ChangeDraft =
  | { kind: "addColumn" }
  | { kind: "renameColumn" | "setDefault" | "setComment" | "dropColumn"; column: ColumnInfo }
  | { kind: "createIndex" }
  | { kind: "dropIndex"; index: string };

const COMMON_TYPES = ["text", "varchar(255)", "integer", "bigint", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
const INDEX_METHODS = ["btree", "hash", "gin", "gist", "brin"];

function Toggle({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string }) {
  return (
    <div className="flex cursor-pointer items-start gap-2.5 select-none" onClick={() => onChange(!checked)}>
      <Checkbox checked={checked} onChange={() => onChange(!checked)} className="mt-0.5" />
      <span>
        <span className="block text-[13px] font-medium">{label}</span>
        {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      </span>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[13px] font-medium">{label}</span>
      {children}
    </label>
  );
}

function ChangeForm({
  schema,
  table,
  draft,
  columns,
  onDone,
  onClose,
}: {
  schema: string;
  table: string;
  draft: ChangeDraft;
  columns: ColumnInfo[];
  onDone: () => void;
  onClose: () => void;
}) {
  const api = useApi();
  const column = "column" in draft ? draft.column : null;
  const [name, setName] = useState(column?.name ?? "");
  const [dataType, setDataType] = useState("text");
  const [nullable, setNullable] = useState(true);
  const [defaultValue, setDefaultValue] = useState(column?.defaultValue ?? "");
  const [comment, setComment] = useState(column?.comment ?? "");
  const [cascade, setCascade] = useState(false);
  const [indexColumns, setIndexColumns] = useState<string[]>([]);
  const [expression, setExpression] = useState("");
  const [unique, setUnique] = useState(false);
  const [method, setMethod] = useState("btree");
  const [concurrently, setConcurrently] = useState(false);
  const [sql, setSql] = useState("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const change: StructureChange = (() => {
    switch (draft.kind) {
      case "addColumn":
        return { kind: "addColumn", name, dataType, nullable, default: defaultValue || null, comment: comment || null };
      case "renameColumn":
        return { kind: "renameColumn", name: column!.name, newName: name };
      case "setDefault":
        return { kind: "setDefault", name: column!.name, default: defaultValue || null };
      case "setComment":
        return { kind: "setComment", name: column!.name, comment: comment || null };
      case "dropColumn":
        return { kind: "dropColumn", name: column!.name, cascade };
      case "createIndex":
        return {
          kind: "createIndex",
          name: name || null,
          columns: [...indexColumns, ...(expression.trim() ? [expression.trim()] : [])],
          unique,
          method,
          concurrently,
        };
      case "dropIndex":
        return { kind: "dropIndex", name: draft.index, cascade };
    }
  })();
  const changeKey = JSON.stringify(change);

  // The generated SQL follows the form as it's filled in
  useEffect(() => {
    let stale = false;
    const timer = setTimeout(() => {
      api
        .previewStructureChange(schema, table, JSON.parse(changeKey))
        .then((text) => !stale && (setSql(text), setPreviewError(null)))
        .catch((err) => !stale && (setSql(""), setPreviewError(typeof err === "string" ? err : "Invalid change")));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, changeKey]);

  async function run() {
    setBusy(true);
    setError(null);
    try {
      await api.applyStructureChange(schema, table, change);
      onDone();
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : "The change failed");
      setBusy(false);
    }
  }

  const destructive = draft.kind === "dropColumn" || draft.kind === "dropIndex";
  const title = {
    addColumn: "Add column",
    renameColumn: `Rename ${column?.name}`,
    setDefault: `Default of ${column?.name}`,
    setComment: `Comment on ${column?.name}`,
    dropColumn: `Drop column ${column?.name}?`,
    createIndex: "Create index",
    dropIndex: `Drop index ${"index" in draft ? draft.index : ""}?`,
  }[draft.kind];
  const icon = destructive ? <Trash2 /> : draft.kind === "createIndex" ? <ListTree /> : draft.kind === "addColumn" ? <Columns3 /> : <PenLine />;

  return (
    <Dialog
      open
      onClose={onClose}
      icon={icon}
      tone={destructive ? "destructive" : "neutral"}
      title={title}
      description={draft.kind === "dropColumn" ? "Its data in every row is deleted. This can't be undone." : `${schema}.${table}`}
      className="max-w-lg"
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={destructive}>
            Cancel
          </Button>
          <Button variant={destructive ? "destructive" : "primary"} size="sm" onClick={run} disabled={!sql || busy}>
            {destructive ? "Drop" : "Apply"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        {(draft.kind === "addColumn" || draft.kind === "renameColumn") && (
          <Field label={draft.kind === "addColumn" ? "Name" : "New name"}>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus spellCheck={false} className="font-mono" />
          </Field>
        )}
        {draft.kind === "addColumn" && (
          <>
            <Field label="Type">
              <Input value={dataType} onChange={(e) => setDataType(e.target.value)} spellCheck={false} className="font-mono" list="dbunny-common-types" />
              <datalist id="dbunny-common-types">
                {COMMON_TYPES.map((t) => (
                  <option key={t} value={t} />
                ))}
              </datalist>
            </Field>
            <Toggle checked={!nullable} onChange={(v) => setNullable(!v)} label="Required (NOT NULL)" hint="Existing rows need a default to satisfy it" />
          </>
        )}
        {(draft.kind === "addColumn" || draft.kind === "setDefault") && (
          <Field label="Default (SQL expression)">
            <Input value={defaultValue} onChange={(e) => setDefaultValue(e.target.value)} placeholder="none, e.g. 0, now(), 'text'" spellCheck={false} className="font-mono" autoFocus={draft.kind === "setDefault"} />
          </Field>
        )}
        {(draft.kind === "addColumn" || draft.kind === "setComment") && (
          <Field label="Comment">
            <Input value={comment} onChange={(e) => setComment(e.target.value)} placeholder="optional" autoFocus={draft.kind === "setComment"} />
          </Field>
        )}
        {draft.kind === "createIndex" && (
          <>
            {/* A div, not a Field label: a label would forward clicks to the first checkbox */}
            <div className="flex flex-col gap-1.5">
              <span className="text-[13px] font-medium">Columns, in order</span>
              <div className="flex max-h-40 flex-col gap-1 overflow-y-auto rounded-lg border border-border p-1.5">
                {columns.map((c) => {
                  const position = indexColumns.indexOf(c.name);
                  return (
                    <div
                      key={c.name}
                      className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-accent"
                      onClick={() => setIndexColumns((prev) => (position >= 0 ? prev.filter((n) => n !== c.name) : [...prev, c.name]))}
                    >
                      <Checkbox checked={position >= 0} onChange={() => setIndexColumns((prev) => (position >= 0 ? prev.filter((n) => n !== c.name) : [...prev, c.name]))} />
                      <span className="flex-1 font-mono text-xs">{c.name}</span>
                      {position >= 0 && <span className="text-[11px] text-muted-foreground tabular-nums">{position + 1}</span>}
                    </div>
                  );
                })}
              </div>
            </div>
            <Field label="Or an expression">
              <Input value={expression} onChange={(e) => setExpression(e.target.value)} placeholder="e.g. lower(email)" spellCheck={false} className="font-mono" />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Name">
                <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="automatic" spellCheck={false} className="font-mono" />
              </Field>
              <Field label="Method">
                <select value={method} onChange={(e) => setMethod(e.target.value)} className={cn(selectClass, "w-full")}>
                  {INDEX_METHODS.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <Toggle checked={unique} onChange={setUnique} label="Unique" hint="Refuses rows that repeat these values" />
            <Toggle checked={concurrently} onChange={setConcurrently} label="Build concurrently" hint="Slower, but doesn't block writes to the table meanwhile" />
          </>
        )}
        {destructive && (
          <Toggle checked={cascade} onChange={setCascade} label="Cascade" hint="Also drops views and constraints that depend on it" />
        )}

        <div>
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">SQL</span>
          <pre className={cn("mt-1 max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-muted px-3 py-2 font-mono text-xs", previewError && "text-muted-foreground")}>
            {sql || previewError || "…"}
          </pre>
        </div>
      </div>
    </Dialog>
  );
}

/** Column and index changes, with the SQL shown before running it. Mounted only while open. */
export default function StructureChangeDialog({ draft, ...props }: { draft: ChangeDraft | null } & Omit<Parameters<typeof ChangeForm>[0], "draft">) {
  return draft ? <ChangeForm draft={draft} {...props} /> : null;
}
