import { useEffect, useState } from "react";
import { Columns3, KeyRound, ListTree, PenLine, ShieldCheck, Trash2 } from "lucide-react";
import type { ColumnGeneration, ColumnInfo, CompletionRelation, StructureChange } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useNotify } from "../../contexts/NoticeProvider";
import { useTab } from "../../contexts/workspace";
import { requestOpenQuery } from "../../lib/openQuery";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import CheckOption from "../ui/CheckOption";
import Button from "../ui/Button";
import Input from "../ui/Input";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { SqlPreviewBox } from "../schema/SqlPreview";
import ForeignKeyEditor from "./ForeignKeyEditor";
import { guessReference } from "../../lib/foreignKeyForm";

/** Which form to show; fields not chosen yet come from the column being changed. */
export type ChangeDraft =
  | { kind: "addColumn" }
  | { kind: "renameColumn" | "setDefault" | "setComment" | "dropColumn"; column: ColumnInfo }
  | { kind: "createIndex" }
  | { kind: "dropIndex"; index: string }
  | { kind: "setType"; column: ColumnInfo }
  | { kind: "addForeignKey" | "addUnique" | "addCheck" }
  | { kind: "dropConstraint"; constraint: string };

const COMMON_TYPES = ["text", "varchar(255)", "integer", "bigint", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
const SQLITE_TYPES = ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"];
const INDEX_METHODS = ["btree", "hash", "gin", "gist", "brin"];
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[13px] font-medium">{label}</span>
      {children}
    </label>
  );
}

/** Columns picked in order (the order matters for indexes and keys). */
function ColumnPicker({ label, columns, value, onChange }: { label: string; columns: ColumnInfo[]; value: string[]; onChange: (next: string[]) => void }) {
  return (
    // A div, not a Field label: a label would forward clicks to the first checkbox
    <div className="flex flex-col gap-1.5">
      <span className="text-[13px] font-medium">{label}</span>
      <div className="flex max-h-40 flex-col gap-1 overflow-y-auto rounded-lg border border-border p-1.5">
        {columns.map((c) => {
          const position = value.indexOf(c.name);
          const toggle = () => onChange(position >= 0 ? value.filter((n) => n !== c.name) : [...value, c.name]);
          return (
            <div key={c.name} className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-accent" onClick={toggle}>
              <Checkbox checked={position >= 0} onChange={toggle} />
              <span className="flex-1 font-mono text-xs">{c.name}</span>
              <span className="font-mono text-[11px] text-muted-foreground">{c.typeName}</span>
              {position >= 0 && <span className="w-3 text-right text-[11px] text-muted-foreground tabular-nums">{position + 1}</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The referenced column a new foreign key column most likely points at. */
const quoteName = (name: string) => (/^[a-z_][a-z0-9_]*$/.test(name) ? name : `"${name.replace(/"/g, '""')}"`);

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
  const { tabId } = useTab();
  const capabilities = useCapabilities();
  const suggestedTypes = useSqlDialect() === "sqlite" ? SQLITE_TYPES : COMMON_TYPES;
  const column = "column" in draft ? draft.column : null;
  const [name, setName] = useState(column?.name ?? "");
  const [dataType, setDataType] = useState(draft.kind === "setType" ? draft.column.typeName : "text");
  const [nullable, setNullable] = useState(true);
  const [columnUnique, setColumnUnique] = useState(false);
  const [columnCheck, setColumnCheck] = useState("");
  const [generation, setGeneration] = useState<ColumnGeneration>("none");
  const [generatedExpr, setGeneratedExpr] = useState("");
  const [defaultValue, setDefaultValue] = useState(column?.defaultValue ?? "");
  const [comment, setComment] = useState(column?.comment ?? "");
  const [cascade, setCascade] = useState(false);
  const [indexColumns, setIndexColumns] = useState<string[]>([]);
  const [expression, setExpression] = useState("");
  const [unique, setUnique] = useState(false);
  const [method, setMethod] = useState("btree");
  const [concurrently, setConcurrently] = useState(false);
  const [using, setUsing] = useState("");
  const [keyColumns, setKeyColumns] = useState<string[]>([]);
  const [relations, setRelations] = useState<CompletionRelation[] | null>(null);
  const [refTable, setRefTable] = useState("");
  const [refColumns, setRefColumns] = useState<Record<string, string>>({});
  const [onUpdate, setOnUpdate] = useState("NO ACTION");
  const [onDelete, setOnDelete] = useState("NO ACTION");
  const [sql, setSql] = useState("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const notify = useNotify();

  // The tables a foreign key can point at, with their columns
  useEffect(() => {
    if (draft.kind !== "addForeignKey") return;
    let stale = false;
    api
      .getCompletionSchema()
      .then((s) => !stale && setRelations(s.relations.filter((r) => r.type === "table")))
      .catch(() => !stale && setRelations([]));
    return () => {
      stale = true;
    };
  }, [api, draft.kind]);
  const target = relations?.find((r) => `${r.schema}.${r.name}` === refTable);

  const change: StructureChange = (() => {
    switch (draft.kind) {
      case "addColumn":
        return {
          kind: "addColumn",
          name,
          dataType,
          nullable,
          default: generation === "none" ? defaultValue || null : null,
          comment: comment || null,
          unique: columnUnique,
          check: columnCheck.trim() || null,
          generation: suggestedTypes === SQLITE_TYPES ? "none" : generation,
          generated: generation === "generated" ? generatedExpr.trim() || null : null,
        };
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
      case "setType":
        return { kind: "setType", name: column!.name, dataType, using: using || null };
      case "addForeignKey":
        return {
          kind: "addForeignKey",
          name: name || null,
          columns: keyColumns,
          refSchema: target?.schema ?? "",
          refTable: target?.name ?? "",
          refColumns: keyColumns.map((c) => refColumns[c] ?? guessReference(c, target)),
          onUpdate,
          onDelete,
        };
      case "addUnique":
        return { kind: "addUnique", name: name || null, columns: keyColumns };
      case "addCheck":
        return { kind: "addCheck", name: name || null, expression };
      case "dropConstraint":
        return { kind: "dropConstraint", name: draft.constraint, cascade };
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
    try {
      await api.applyStructureChange(schema, table, change);
      onDone();
      onClose();
    } catch (err) {
      notify({ tone: "error", text: typeof err === "string" ? err : "The change failed" });
      setBusy(false);
    }
  }

  const destructive = draft.kind === "dropColumn" || draft.kind === "dropIndex" || draft.kind === "dropConstraint";
  const title = {
    addColumn: "Add column",
    renameColumn: `Rename ${column?.name}`,
    setDefault: `Default of ${column?.name}`,
    setComment: `Comment on ${column?.name}`,
    dropColumn: `Drop column ${column?.name}?`,
    createIndex: "Create index",
    dropIndex: `Drop index ${"index" in draft ? draft.index : ""}?`,
    setType: `Type of ${column?.name}`,
    addForeignKey: "Add foreign key",
    addUnique: "Add unique constraint",
    addCheck: "Add check constraint",
    dropConstraint: `Drop constraint ${"constraint" in draft ? draft.constraint : ""}?`,
  }[draft.kind];
  const icon = destructive ? (
    <Trash2 />
  ) : draft.kind === "createIndex" ? (
    <ListTree />
  ) : draft.kind === "addColumn" ? (
    <Columns3 />
  ) : draft.kind === "addForeignKey" ? (
    <KeyRound />
  ) : draft.kind === "addUnique" || draft.kind === "addCheck" ? (
    <ShieldCheck />
  ) : (
    <PenLine />
  );
  const description =
    draft.kind === "dropColumn"
      ? "Its data in every row is deleted. This can't be undone."
      : draft.kind === "setType"
        ? `Now ${column?.typeName}. Every row is converted, and the table is locked meanwhile.`
        : `${schema}.${table}`;

  return (
    <Dialog
      open
      onClose={onClose}
      icon={icon}
      tone={destructive ? "destructive" : "neutral"}
      title={title}
      description={description}
      className="max-w-lg"
      footer={
        <>
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
        {draft.kind === "setType" && (
          <>
            <Field label="New type">
              <Input value={dataType} onChange={(e) => setDataType(e.target.value)} autoFocus spellCheck={false} className="font-mono" list="dbunny-common-types" />
            </Field>
            <Field label="Conversion (USING)">
              <Input
                value={using}
                onChange={(e) => setUsing(e.target.value)}
                placeholder={`automatic, e.g. ${quoteName(column!.name)}::integer`}
                spellCheck={false}
                className="font-mono"
              />
            </Field>
          </>
        )}
        {draft.kind === "addColumn" && (
          <>
            <Field label="Type">
              <Input value={dataType} onChange={(e) => setDataType(e.target.value)} spellCheck={false} className="font-mono" list="dbunny-common-types" />
            </Field>
            {suggestedTypes !== SQLITE_TYPES && (
              <>
                <Field label="Generation">
                  <select value={generation} onChange={(e) => setGeneration(e.target.value as ColumnGeneration)} className={cn(selectClass, "w-full")}>
                    <option value="none">None</option>
                    <option value="identityByDefault">Identity, by default</option>
                    <option value="identityAlways">Identity, always</option>
                    <option value="generated">Generated</option>
                  </select>
                </Field>
                {generation === "generated" && (
                  <Field label="Generated expression">
                    <Input value={generatedExpr} onChange={(e) => setGeneratedExpr(e.target.value)} placeholder="e.g. price * 2" spellCheck={false} className="font-mono" />
                  </Field>
                )}
              </>
            )}
            <CheckOption checked={!nullable} onChange={(v) => setNullable(!v)} label="NOT NULL" hint="Existing rows need a default to satisfy it" />
            <CheckOption checked={columnUnique} onChange={setColumnUnique} label="Unique" hint="No two rows may share this value" />
            <Field label="Check (SQL expression)">
              <Input value={columnCheck} onChange={(e) => setColumnCheck(e.target.value)} placeholder="none, e.g. price >= 0" spellCheck={false} className="font-mono" />
            </Field>
          </>
        )}
        {(draft.kind === "addColumn" || draft.kind === "setDefault") && (
          <Field label="Default (SQL expression)">
            <Input value={draft.kind === "addColumn" && generation !== "none" ? "" : defaultValue} onChange={(e) => setDefaultValue(e.target.value)} placeholder="none, e.g. 0, now(), 'text'" disabled={draft.kind === "addColumn" && generation !== "none"} spellCheck={false} className="font-mono" autoFocus={draft.kind === "setDefault"} />
          </Field>
        )}
        {((draft.kind === "addColumn" && capabilities.comments) || draft.kind === "setComment") && (
          <Field label="Comment">
            <Input value={comment} onChange={(e) => setComment(e.target.value)} placeholder="optional" autoFocus={draft.kind === "setComment"} />
          </Field>
        )}
        {draft.kind === "createIndex" && (
          <>
            <ColumnPicker label="Columns, in order" columns={columns} value={indexColumns} onChange={setIndexColumns} />
            <Field label="Or an expression">
              <Input value={expression} onChange={(e) => setExpression(e.target.value)} placeholder="e.g. lower(email)" spellCheck={false} className="font-mono" />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Name">
                <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="automatic" spellCheck={false} className="font-mono" />
              </Field>
              {capabilities.indexMethods && (
                <Field label="Method">
                  <select value={method} onChange={(e) => setMethod(e.target.value)} className={cn(selectClass, "w-full")}>
                    {INDEX_METHODS.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </Field>
              )}
            </div>
            <CheckOption checked={unique} onChange={setUnique} label="Unique" hint="Refuses rows that repeat these values" />
            {capabilities.concurrentIndex && (
              <CheckOption checked={concurrently} onChange={setConcurrently} label="Build concurrently" hint="Slower, but doesn't block writes to the table meanwhile" />
            )}
          </>
        )}
        {(draft.kind === "addUnique" || draft.kind === "addForeignKey") && (
          <ColumnPicker label={draft.kind === "addUnique" ? "Columns that can't repeat together" : "Columns"} columns={columns} value={keyColumns} onChange={setKeyColumns} />
        )}
        {draft.kind === "addForeignKey" && (
          <ForeignKeyEditor
            relations={relations}
            value={{ refTable, columns: keyColumns, refColumns, onUpdate, onDelete }}
            onChange={(patch) => {
              if (patch.refTable !== undefined) (setRefTable(patch.refTable), setRefColumns({}));
              if (patch.refColumns !== undefined) setRefColumns(patch.refColumns);
              if (patch.onUpdate !== undefined) setOnUpdate(patch.onUpdate);
              if (patch.onDelete !== undefined) setOnDelete(patch.onDelete);
            }}
            localColumnNames={keyColumns}
          />
        )}
        {draft.kind === "addCheck" && (
          <Field label="Rows must satisfy">
            <Input value={expression} onChange={(e) => setExpression(e.target.value)} placeholder="e.g. price >= 0" autoFocus spellCheck={false} className="font-mono" />
          </Field>
        )}
        {(draft.kind === "addForeignKey" || draft.kind === "addUnique" || draft.kind === "addCheck") && (
          <Field label="Name">
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="automatic" spellCheck={false} className="font-mono" />
          </Field>
        )}
        {destructive && capabilities.cascade && (
          <CheckOption checked={cascade} onChange={setCascade} label="Cascade" hint="Also drops views and constraints that depend on it" />
        )}

        <datalist id="dbunny-common-types">
          {suggestedTypes.map((t) => (
            <option key={t} value={t} />
          ))}
        </datalist>
        <SqlPreviewBox
          source={sql}
          error={previewError}
          onOpen={() => {
            if (!sql) return;
            requestOpenQuery(tabId, sql);
            onClose();
          }}
        />
      </div>
    </Dialog>
  );
}

/** Column, index and constraint changes, with the SQL shown before running it. Mounted only while open. */
export default function StructureChangeDialog({ draft, ...props }: { draft: ChangeDraft | null } & Omit<Parameters<typeof ChangeForm>[0], "draft">) {
  return draft ? <ChangeForm draft={draft} {...props} /> : null;
}
