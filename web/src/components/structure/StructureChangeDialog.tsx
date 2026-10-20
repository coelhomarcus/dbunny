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
import { dialectData } from "../../lib/engines";
import { SqlPreviewBox } from "../schema/SqlPreview";
import ForeignKeyEditor from "./ForeignKeyEditor";
import { guessReference } from "../../lib/foreignKeyForm";
import { useT, type StaticKey } from "../../i18n";

/** Which form to show; fields not chosen yet come from the column being changed. */
export type ChangeDraft =
  | { kind: "addColumn" }
  | { kind: "renameColumn" | "setDefault" | "setComment" | "dropColumn"; column: ColumnInfo }
  | { kind: "createIndex" }
  | { kind: "dropIndex"; index: string }
  | { kind: "setType"; column: ColumnInfo }
  | { kind: "addForeignKey" | "addUnique" | "addCheck" }
  | { kind: "dropConstraint"; constraint: string };

/** How each way of producing a column is named when adding one. */
const GENERATION_LABELS: Record<ColumnGeneration, StaticKey> = {
  none: "structure.structureChangeDialog.genNone",
  autoincrement: "structure.structureChangeDialog.genAutoincrement",
  identityByDefault: "structure.structureChangeDialog.genIdentityByDefault",
  identityAlways: "structure.structureChangeDialog.genIdentityAlways",
  generated: "structure.structureChangeDialog.genGenerated",
};
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
  const t = useT();
  const { tabId } = useTab();
  const capabilities = useCapabilities();
  const data = dialectData(useSqlDialect());
  const suggestedTypes = data.suggestedTypes;
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
  const [method, setMethod] = useState(data.defaultIndexMethod);
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
          generation: data.addColumnGenerations.length > 0 ? generation : "none",
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
        .catch((err) => !stale && (setSql(""), setPreviewError(typeof err === "string" ? err : t("structure.structureChangeDialog.invalidChange"))));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, changeKey, t]);

  async function run() {
    setBusy(true);
    try {
      await api.applyStructureChange(schema, table, change);
      onDone();
      onClose();
    } catch (err) {
      notify({ tone: "error", text: typeof err === "string" ? err : t("structure.structureChangeDialog.changeFailed") });
      setBusy(false);
    }
  }

  const destructive = draft.kind === "dropColumn" || draft.kind === "dropIndex" || draft.kind === "dropConstraint";
  const title = {
    addColumn: t("structure.structureChangeDialog.titleAddColumn"),
    renameColumn: t("structure.structureChangeDialog.titleRenameColumn", { name: column?.name ?? "" }),
    setDefault: t("structure.structureChangeDialog.titleSetDefault", { name: column?.name ?? "" }),
    setComment: t("structure.structureChangeDialog.titleSetComment", { name: column?.name ?? "" }),
    dropColumn: t("structure.structureChangeDialog.titleDropColumn", { name: column?.name ?? "" }),
    createIndex: t("structure.structureChangeDialog.titleCreateIndex"),
    dropIndex: t("structure.structureChangeDialog.titleDropIndex", { name: "index" in draft ? draft.index : "" }),
    setType: t("structure.structureChangeDialog.titleSetType", { name: column?.name ?? "" }),
    addForeignKey: t("structure.structureChangeDialog.titleAddForeignKey"),
    addUnique: t("structure.structureChangeDialog.titleAddUnique"),
    addCheck: t("structure.structureChangeDialog.titleAddCheck"),
    dropConstraint: t("structure.structureChangeDialog.titleDropConstraint", { name: "constraint" in draft ? draft.constraint : "" }),
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
      ? t("structure.structureChangeDialog.dropColumnNote")
      : draft.kind === "setType"
        ? t("structure.structureChangeDialog.setTypeNote", { type: column?.typeName ?? "" })
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
            {t("common.cancel")}
          </Button>
          <Button variant={destructive ? "destructive" : "primary"} size="sm" onClick={run} disabled={!sql || busy}>
            {destructive ? t("structure.structureChangeDialog.drop") : t("structure.structureChangeDialog.apply")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        {(draft.kind === "addColumn" || draft.kind === "renameColumn") && (
          <Field label={draft.kind === "addColumn" ? t("structure.structureChangeDialog.name") : t("structure.structureChangeDialog.newName")}>
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus spellCheck={false} className="font-mono" />
          </Field>
        )}
        {draft.kind === "setType" && (
          <>
            <Field label={t("structure.structureChangeDialog.newType")}>
              <Input value={dataType} onChange={(e) => setDataType(e.target.value)} autoFocus spellCheck={false} className="font-mono" list="dbunny-common-types" />
            </Field>
            <Field label={t("structure.structureChangeDialog.conversion")}>
              <Input
                value={using}
                onChange={(e) => setUsing(e.target.value)}
                placeholder={t("structure.structureChangeDialog.usingPlaceholder", { example: `${quoteName(column!.name)}::integer` })}
                spellCheck={false}
                className="font-mono"
              />
            </Field>
          </>
        )}
        {draft.kind === "addColumn" && (
          <>
            <Field label={t("structure.structureChangeDialog.type")}>
              <Input value={dataType} onChange={(e) => setDataType(e.target.value)} spellCheck={false} className="font-mono" list="dbunny-common-types" />
            </Field>
            {data.addColumnGenerations.length > 0 && (
              <>
                <Field label={t("structure.structureChangeDialog.generation")}>
                  <select value={generation} onChange={(e) => setGeneration(e.target.value as ColumnGeneration)} className={cn(selectClass, "w-full")}>
                    {data.addColumnGenerations.map((option) => (
                      <option key={option} value={option}>
                        {t(GENERATION_LABELS[option])}
                      </option>
                    ))}
                  </select>
                </Field>
                {generation === "generated" && (
                  <Field label={t("structure.structureChangeDialog.generatedExpression")}>
                    <Input value={generatedExpr} onChange={(e) => setGeneratedExpr(e.target.value)} placeholder={t("structure.structureChangeDialog.expressionExample")} spellCheck={false} className="font-mono" />
                  </Field>
                )}
              </>
            )}
            <CheckOption checked={!nullable} onChange={(v) => setNullable(!v)} label="NOT NULL" hint={t("structure.structureChangeDialog.notNullHint")} />
            <CheckOption checked={columnUnique} onChange={setColumnUnique} label={t("structure.structureChangeDialog.unique")} hint={t("structure.structureChangeDialog.uniqueColumnHint")} />
            <Field label={t("structure.structureChangeDialog.check")}>
              <Input value={columnCheck} onChange={(e) => setColumnCheck(e.target.value)} placeholder={t("structure.structureChangeDialog.checkPlaceholder")} spellCheck={false} className="font-mono" />
            </Field>
          </>
        )}
        {(draft.kind === "addColumn" || draft.kind === "setDefault") && (
          <Field label={t("structure.structureChangeDialog.default")}>
            <Input value={draft.kind === "addColumn" && generation !== "none" ? "" : defaultValue} onChange={(e) => setDefaultValue(e.target.value)} placeholder={t("structure.structureChangeDialog.defaultPlaceholder")} disabled={draft.kind === "addColumn" && generation !== "none"} spellCheck={false} className="font-mono" autoFocus={draft.kind === "setDefault"} />
          </Field>
        )}
        {((draft.kind === "addColumn" && capabilities.comments) || draft.kind === "setComment") && (
          <Field label={t("structure.structureChangeDialog.comment")}>
            <Input value={comment} onChange={(e) => setComment(e.target.value)} placeholder={t("structure.structureChangeDialog.optional")} autoFocus={draft.kind === "setComment"} />
          </Field>
        )}
        {draft.kind === "createIndex" && (
          <>
            <ColumnPicker label={t("structure.structureChangeDialog.columnsInOrder")} columns={columns} value={indexColumns} onChange={setIndexColumns} />
            <Field label={t("structure.structureChangeDialog.orExpression")}>
              <Input value={expression} onChange={(e) => setExpression(e.target.value)} placeholder={t("structure.structureChangeDialog.indexExpressionExample")} spellCheck={false} className="font-mono" />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t("structure.structureChangeDialog.name")}>
                <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("structure.structureChangeDialog.automatic")} spellCheck={false} className="font-mono" />
              </Field>
              {capabilities.indexMethods && (
                <Field label={t("structure.structureChangeDialog.method")}>
                  <select value={method} onChange={(e) => setMethod(e.target.value)} className={cn(selectClass, "w-full")}>
                    {data.indexMethods.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </Field>
              )}
            </div>
            <CheckOption checked={unique} onChange={setUnique} label={t("structure.structureChangeDialog.unique")} hint={t("structure.structureChangeDialog.uniqueIndexHint")} />
            {capabilities.concurrentIndex && (
              <CheckOption checked={concurrently} onChange={setConcurrently} label={t("structure.structureChangeDialog.concurrently")} hint={t("structure.structureChangeDialog.concurrentlyHint")} />
            )}
          </>
        )}
        {(draft.kind === "addUnique" || draft.kind === "addForeignKey") && (
          <ColumnPicker label={draft.kind === "addUnique" ? t("structure.structureChangeDialog.columnsUnique") : t("structure.structureChangeDialog.columns")} columns={columns} value={keyColumns} onChange={setKeyColumns} />
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
          <Field label={t("structure.structureChangeDialog.rowsMustSatisfy")}>
            <Input value={expression} onChange={(e) => setExpression(e.target.value)} placeholder={t("structure.structureChangeDialog.checkExample")} autoFocus spellCheck={false} className="font-mono" />
          </Field>
        )}
        {(draft.kind === "addForeignKey" || draft.kind === "addUnique" || draft.kind === "addCheck") && (
          <Field label={t("structure.structureChangeDialog.name")}>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("structure.structureChangeDialog.automatic")} spellCheck={false} className="font-mono" />
          </Field>
        )}
        {destructive && capabilities.cascade && (
          <CheckOption checked={cascade} onChange={setCascade} label={t("structure.structureChangeDialog.cascade")} hint={t("structure.structureChangeDialog.cascadeHint")} />
        )}

        <datalist id="dbunny-common-types">
          {suggestedTypes.map((type) => (
            <option key={type} value={type} />
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
