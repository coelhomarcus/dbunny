import { useEffect, useRef, useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import type { ColumnGeneration, CompletionRelation, NewColumn, NewTable } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useNotify } from "../../contexts/NoticeProvider";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useTab } from "../../contexts/workspace";
import { foreignKeyDraftError, foreignKeysForSpec, guessReference, REFERENTIAL_ACTIONS, type ForeignKeyDraft } from "../../lib/foreignKeyForm";
import { requestSchemaRefresh } from "../../lib/schemaRefresh";
import { requestOpenQuery } from "../../lib/openQuery";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { dialectData, generationOptions, isGenerated, type DialectData } from "../../lib/engines";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Input from "../ui/Input";
import CheckOption from "../ui/CheckOption";
import Checkbox from "../Checkbox";
import SegmentedTabs from "../ui/SegmentedTabs";
import { selectClass } from "../ui/styles";
import { SqlPreviewBox } from "../schema/SqlPreview";
import { useT, type StaticKey } from "../../i18n";
import { Rich } from "../../i18n/Rich";
import { errorText } from "../../lib/appError";

const PARTITION_METHODS = ["RANGE", "LIST", "HASH"];

/** How each way of producing a column is named in the generation menu. */
const GENERATION_LABELS: Record<ColumnGeneration, StaticKey> = {
  none: "ui.createTablePage.genNone",
  autoincrement: "ui.createTablePage.genAutoincrement",
  identityByDefault: "ui.createTablePage.genIdentityByDefault",
  identityAlways: "ui.createTablePage.genIdentityAlways",
  generated: "ui.createTablePage.genGenerated",
  generatedVirtual: "ui.createTablePage.genGeneratedVirtual",
};

type ForeignKey = ForeignKeyDraft & { key: number };
type Row = NewColumn & { key: number };
type Mode = "simple" | "advanced";

const cellClass =
  "h-9 border-0 bg-transparent px-2.5 shadow-none focus-visible:border-transparent focus-visible:ring-0 dark:bg-transparent";

/** A full page for a new table: a short column list, or the full set of options. */
export default function CreateTablePage({ documentId, schema: initialSchema }: { documentId: string; schema: string }) {
  const t = useT();
  const api = useApi();
  const { tabId } = useTab();
  const { openDocument, closeDocument, updateCreateTable, setQueryDirty } = useDocuments();
  const dialect = useSqlDialect();
  const capabilities = useCapabilities();
  const data = dialectData(dialect);
  const suggestedTypes = data.newTableTypes;
  const hasFlag = (flag: DialectData["createTableFlags"][number]) => data.createTableFlags.includes(flag);
  const nextKey = useRef(0);
  const newRow = (patch: Partial<NewColumn> = {}): Row => ({
    key: nextKey.current++,
    name: "",
    dataType: data.defaultTextType,
    nullable: true,
    primaryKey: false,
    default: null,
    unique: false,
    check: null,
    comment: null,
    generation: "none",
    generated: null,
    ...patch,
  });
  const [schema, setSchema] = useState(initialSchema);
  const [table, setTable] = useState("");
  const [mode, setMode] = useState<Mode>("simple");
  const [rows, setRows] = useState<Row[]>(() =>
    data.starterColumns.map((c) => newRow({ name: c.name, dataType: c.dataType, nullable: c.nullable, primaryKey: c.primaryKey, default: c.default ?? null, generation: c.generation ?? "none" })),
  );
  const [foreignKeys, setForeignKeys] = useState<ForeignKey[]>([]);
  const [relations, setRelations] = useState<CompletionRelation[] | null>(null);
  const [unlogged, setUnlogged] = useState(false);
  const [withoutRowid, setWithoutRowid] = useState(false);
  const [strict, setStrict] = useState(false);
  const [partitionMethod, setPartitionMethod] = useState("");
  const [partitionExpr, setPartitionExpr] = useState("");
  const [sql, setSql] = useState("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const notify = useNotify();

  useEffect(() => {
    updateCreateTable(documentId, { title: table, schema });
  }, [documentId, table, schema, updateCreateTable]);

  const initialForm = useRef<string | null>(null);
  useEffect(() => {
    const key = JSON.stringify({ schema, table, rows, foreignKeys, unlogged, withoutRowid, strict, partitionMethod, partitionExpr });
    if (initialForm.current === null) {
      initialForm.current = key;
      return;
    }
    setQueryDirty(documentId, key !== initialForm.current);
  }, [documentId, schema, table, rows, foreignKeys, unlogged, withoutRowid, strict, partitionMethod, partitionExpr, setQueryDirty]);

  useEffect(() => {
    let stale = false;
    api
      .getCompletionSchema()
      .then((s) => !stale && setRelations(s.relations.filter((r) => r.type === "table")))
      .catch(() => !stale && setRelations([]));
    return () => {
      stale = true;
    };
  }, [api]);

  const columns: NewColumn[] = rows.map((r) => ({
    name: r.name,
    dataType: r.dataType,
    nullable: r.nullable,
    primaryKey: r.primaryKey,
    unique: r.unique,
    check: r.check?.trim() ? r.check : null,
    comment: capabilities.comments && r.comment?.trim() ? r.comment : null,
    generation: r.generation,
    generated: isGenerated(r.generation) && r.generated?.trim() ? r.generated : null,
    default: r.generation === "none" && r.default?.trim() ? r.default : null,
  }));
  const named = columns.map((c) => c.name.trim()).filter(Boolean);
  const fkErrors = foreignKeys.map((fk) => foreignKeyDraftError(fk, named));
  const hasIncompleteForeignKey = fkErrors.some(Boolean);
  const partitionKey = partitionExpr.trim();
  const partitionIncomplete = hasFlag("partition") && !!partitionMethod && !partitionKey;
  const spec: NewTable = {
    columns,
    foreignKeys: foreignKeysForSpec(foreignKeys, relations, named),
    unlogged: hasFlag("unlogged") && unlogged,
    withoutRowid: hasFlag("withoutRowid") && withoutRowid,
    strict: hasFlag("strict") && strict,
    partition: hasFlag("partition") && partitionMethod && partitionKey ? { method: partitionMethod, expression: partitionKey } : null,
  };
  const specKey = JSON.stringify(spec);

  useEffect(() => {
    if (!schema.trim()) return;
    let stale = false;
    const timer = setTimeout(() => {
      api
        .previewCreateTable(schema.trim(), table, JSON.parse(specKey) as NewTable)
        .then((text) => !stale && (setSql(text), setPreviewError(null)))
        .catch((err) => !stale && (setSql(""), setPreviewError(errorText(err, t("ui.createTablePage.invalidTable")))));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, specKey, t]);

  const update = (key: number, patch: Partial<NewColumn>) => setRows((prev) => prev.map((r) => (r.key === key ? { ...r, ...patch } : r)));
  const advanced = mode === "advanced";
  const grid = advanced
    ? capabilities.comments
      ? "grid-cols-[minmax(8rem,1.15fr)_minmax(6.5rem,0.9fr)_4.25rem_minmax(6.5rem,0.9fr)_minmax(6.5rem,0.9fr)_4.25rem_4.25rem_9rem_minmax(7rem,1fr)_2rem]"
      : "grid-cols-[minmax(8rem,1.2fr)_minmax(6.5rem,0.9fr)_4.25rem_minmax(6.5rem,1fr)_4.25rem_4.25rem_9rem_minmax(7rem,1fr)_2rem]"
    : capabilities.comments
      ? "grid-cols-[minmax(0,1.3fr)_minmax(7rem,1fr)_4.5rem_minmax(0,1fr)_minmax(0,1fr)_4.5rem_2rem]"
      : "grid-cols-[minmax(0,1.4fr)_minmax(7rem,1fr)_4.5rem_minmax(0,1fr)_4.5rem_2rem]";

  async function create() {
    if (fkErrors.some(Boolean) || partitionIncomplete) return;
    const name = table.trim();
    const schemaName = schema.trim();
    if (!name || !schemaName) return;
    setBusy(true);
    try {
      await api.createTable(schemaName, name, spec);
      requestSchemaRefresh(tabId, schemaName);
      openDocument({ kind: "data", schema: schemaName, name, icon: "table" });
      closeDocument(documentId);
    } catch (err) {
      notify({ tone: "error", text: errorText(err, t("ui.createTablePage.createFailed")) });
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
        <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
          <div className="flex flex-col gap-2">
            <Field label={t("ui.createTablePage.schema")}>
              <Input
                value={schema}
                onChange={(e) => setSchema(e.target.value)}
                readOnly={!capabilities.schemas}
                spellCheck={false}
                aria-label={t("ui.createTablePage.schema")}
                className="h-9 border-0 bg-foreground/[0.04] font-medium shadow-none focus-visible:ring-1 dark:bg-foreground/[0.04]"
              />
            </Field>
            <Field label={t("ui.createTablePage.tableName")}>
              <Input
                value={table}
                onChange={(e) => setTable(e.target.value)}
                placeholder={t("ui.createTablePage.tableNamePlaceholder")}
                autoFocus
                spellCheck={false}
                aria-label={t("ui.createTablePage.tableName")}
                className="h-9 border-0 bg-foreground/[0.04] font-medium shadow-none focus-visible:ring-1 dark:bg-foreground/[0.04]"
              />
            </Field>
          </div>

          <div>
            <div className="mb-3 flex items-center gap-3">
              <h2 className="text-sm font-semibold">{t("ui.createTablePage.columns")}</h2>
              <SegmentedTabs
                value={mode}
                onChange={setMode}
                options={[
                  { value: "simple", label: t("ui.createTablePage.simple") },
                  { value: "advanced", label: t("ui.createTablePage.advanced") },
                ]}
              />
              <Button
                variant="primary"
                size="icon-sm"
                aria-label={t("ui.createTablePage.addColumn")}
                className="ml-auto rounded-full"
                onClick={() => {
                  // The key is taken here, not inside the updater, which react may run twice
                  const row = newRow();
                  setRows((prev) => [...prev, row]);
                }}
              >
                <Plus />
              </Button>
            </div>

            <div className={advanced ? "overflow-x-auto" : undefined}>
            <div className={advanced ? "min-w-[64rem]" : undefined}>
            <div className={cn("grid items-center px-1 pb-1 text-[11px] text-muted-foreground", grid)}>
              <span className="px-2.5">{t("ui.createTablePage.name")}</span>
              <span className="px-2.5">{t("ui.createTablePage.type")}</span>
              <span className="text-center">{t("ui.createTablePage.nullable")}</span>
              <span className="px-2.5">{t("ui.createTablePage.defaultValue")}</span>
              {capabilities.comments && <span className="px-2.5">{t("ui.createTablePage.comment")}</span>}
              <span className="text-center">{t("ui.createTablePage.primary")}</span>
              {advanced && <span className="text-center">{t("ui.createTablePage.unique")}</span>}
              {advanced && <span className="px-2.5">{t("ui.createTablePage.generation")}</span>}
              {advanced && <span className="px-2.5">{t("ui.createTablePage.check")}</span>}
              <span />
            </div>

            <div className="flex flex-col gap-1">
              {rows.map((r) => (
                <div key={r.key} className="rounded-md bg-foreground/[0.045]">
                  <div className={cn("grid items-center", grid)}>
                    <Input value={r.name} onChange={(e) => update(r.key, { name: e.target.value })} aria-label={t("ui.createTablePage.columnName")} placeholder={t("ui.createTablePage.namePlaceholder")} spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    <Input value={r.dataType} onChange={(e) => update(r.key, { dataType: e.target.value })} aria-label={t("ui.createTablePage.type")} spellCheck={false} list={`dbunny-new-table-types-${documentId}`} className={cn(cellClass, "font-mono text-[13px]")} />
                    <span className="flex justify-center">
                      <Checkbox
                        checked={r.nullable && !r.primaryKey}
                        disabled={r.primaryKey}
                        onChange={() => !r.primaryKey && update(r.key, { nullable: !r.nullable })}
                      />
                    </span>
                    <Input
                      value={r.generation === "none" ? (r.default ?? "") : ""}
                      onChange={(e) => update(r.key, { default: e.target.value })}
                      aria-label={t("ui.createTablePage.defaultValue")}
                      placeholder="(NULL)"
                      disabled={r.generation !== "none"}
                      spellCheck={false}
                      className={cn(cellClass, "font-mono text-[13px]")}
                    />
                    {capabilities.comments && (
                      <Input value={r.comment ?? ""} onChange={(e) => update(r.key, { comment: e.target.value })} aria-label={t("ui.createTablePage.comment")} placeholder="(NULL)" className={cn(cellClass, "text-[13px]")} />
                    )}
                    <span className="flex justify-center">
                      <Checkbox
                        checked={r.primaryKey}
                        onChange={() => update(r.key, { primaryKey: !r.primaryKey, nullable: r.primaryKey ? r.nullable : false })}
                      />
                    </span>
                    {advanced && (
                      <span className="flex justify-center">
                        <Checkbox checked={r.unique} onChange={() => update(r.key, { unique: !r.unique })} />
                      </span>
                    )}
                    {advanced && (
                      <select
                        aria-label={t("ui.createTablePage.generation")}
                        value={r.generation}
                        onChange={(e) => update(r.key, { generation: e.target.value as ColumnGeneration })}
                        className="h-9 w-full cursor-pointer bg-transparent px-2 text-[13px] outline-none"
                      >
                        {generationOptions(data.newTableGenerations, capabilities.virtualGenerated).map((option) => (
                          <option key={option} value={option}>
                            {t(GENERATION_LABELS[option])}
                          </option>
                        ))}
                      </select>
                    )}
                    {advanced && (
                      <Input value={r.check ?? ""} onChange={(e) => update(r.key, { check: e.target.value })} aria-label={t("ui.createTablePage.check")} placeholder={t("ui.createTablePage.checkPlaceholder")} spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    )}
                    <span className="flex justify-center">
                      <Button variant="ghost-muted" size="icon-xs" aria-label={t("ui.createTablePage.removeColumn", { name: r.name || t("ui.createTablePage.columnFallback") })} onClick={() => setRows((prev) => prev.filter((p) => p.key !== r.key))} disabled={rows.length === 1}>
                        <X />
                      </Button>
                    </span>
                  </div>
                  {advanced && isGenerated(r.generation) && (
                    <div className="grid grid-cols-[7.5rem_1fr] items-center border-t border-border/50">
                      <span className="px-2.5 text-[11px] text-muted-foreground">{t("ui.createTablePage.expression")}</span>
                      <Input value={r.generated ?? ""} onChange={(e) => update(r.key, { generated: e.target.value })} aria-label={t("ui.createTablePage.generatedExpression")} placeholder={t("ui.createTablePage.expressionPlaceholder")} spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    </div>
                  )}
                </div>
              ))}
            </div>
            </div>
            </div>
            <datalist id={`dbunny-new-table-types-${documentId}`}>
              {suggestedTypes.map((type) => (
                <option key={type} value={type} />
              ))}
            </datalist>
          </div>

          {advanced && (
            <>
              <section>
                <div className="mb-3 flex items-center">
                  <h2 className="text-sm font-semibold">{t("ui.createTablePage.foreignKeys")}</h2>
                  <Button
                    variant="primary"
                    size="icon-sm"
                    aria-label={t("ui.createTablePage.addForeignKey")}
                    className="ml-auto rounded-full"
                    onClick={() => {
                      const key = nextKey.current++;
                      setForeignKeys((prev) => [...prev, { key, columns: [], refTable: "", refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" }]);
                    }}
                  >
                    <Plus />
                  </Button>
                </div>
                {foreignKeys.length > 0 && (
                  <div className="flex flex-col gap-1">
                    {foreignKeys.map((fk, index) => {
                      const fkError = fkErrors[index];
                      const patch = (next: Partial<ForeignKeyDraft>) =>
                        setForeignKeys((prev) => prev.map((item) => (item.key === fk.key ? { ...item, ...next } : item)));
                      const target = relations?.find((r) => `${r.schema}.${r.name}` === fk.refTable);
                      const keyColumns = fk.columns.filter((c) => named.includes(c));
                      return (
                        <div key={fk.key} className={cn("rounded-md bg-foreground/[0.045] px-3 py-2.5", fkError && "ring-1 ring-destructive")}>
                          <div className="mb-2 flex items-start gap-2">
                            <div className="flex min-w-0 flex-1 flex-wrap gap-1.5">
                              {named.length === 0 ? (
                                <span className="text-[13px] text-muted-foreground">{t("ui.createTablePage.nameAColumn")}</span>
                              ) : (
                                named.map((name) => {
                                  const on = fk.columns.includes(name);
                                  const toggle = () => patch({ columns: on ? fk.columns.filter((c) => c !== name) : [...fk.columns, name] });
                                  return (
                                    <div
                                      key={name}
                                      onClick={toggle}
                                      className={cn(
                                        "flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2 font-mono text-xs",
                                        on ? "bg-primary/15 text-foreground" : "bg-foreground/[0.05] text-muted-foreground hover:text-foreground",
                                      )}
                                    >
                                      <Checkbox checked={on} onChange={toggle} />
                                      {name}
                                    </div>
                                  );
                                })
                              )}
                            </div>
                            <Button variant="ghost-muted" size="icon-xs" aria-label={t("ui.createTablePage.removeForeignKey")} onClick={() => setForeignKeys((prev) => prev.filter((item) => item.key !== fk.key))}>
                              <X />
                            </Button>
                          </div>
                          <div className="grid gap-2 sm:grid-cols-3">
                            <select
                              aria-label={t("ui.createTablePage.references")}
                              value={fk.refTable}
                              onChange={(e) => patch({ refTable: e.target.value, refColumns: {} })}
                              className={cn(selectClass, "w-full")}
                            >
                              <option value="">{relations === null ? t("common.loading") : t("ui.createTablePage.references")}</option>
                              {(relations ?? []).map((r) => (
                                <option key={`${r.schema}.${r.name}`} value={`${r.schema}.${r.name}`}>
                                  {r.schema}.{r.name}
                                </option>
                              ))}
                            </select>
                            <select aria-label={t("ui.createTablePage.onDelete")} value={fk.onDelete} onChange={(e) => patch({ onDelete: e.target.value })} className={cn(selectClass, "w-full")}>
                              {REFERENTIAL_ACTIONS.map((action) => (
                                <option key={action} value={action}>
                                  {t("ui.createTablePage.onDeleteAction", { action: action.toLowerCase() })}
                                </option>
                              ))}
                            </select>
                            <select aria-label={t("ui.createTablePage.onUpdate")} value={fk.onUpdate} onChange={(e) => patch({ onUpdate: e.target.value })} className={cn(selectClass, "w-full")}>
                              {REFERENTIAL_ACTIONS.map((action) => (
                                <option key={action} value={action}>
                                  {t("ui.createTablePage.onUpdateAction", { action: action.toLowerCase() })}
                                </option>
                              ))}
                            </select>
                          </div>
                          {target && keyColumns.length > 0 && (
                            <div className="mt-2 grid gap-2 sm:grid-cols-2">
                              {keyColumns.map((column) => (
                                <label key={column} className="flex items-center gap-2">
                                  <span className="w-24 shrink-0 truncate font-mono text-xs">{column}</span>
                                  <select
                                    aria-label={t("ui.createTablePage.referencedColumn", { column })}
                                    value={fk.refColumns[column] ?? guessReference(column, target)}
                                    onChange={(e) => patch({ refColumns: { ...fk.refColumns, [column]: e.target.value } })}
                                    className={cn(selectClass, "min-w-0 flex-1 font-mono text-xs")}
                                  >
                                    {target.columns.map((rc) => (
                                      <option key={rc.name} value={rc.name}>
                                        {rc.name}
                                      </option>
                                    ))}
                                  </select>
                                </label>
                              ))}
                            </div>
                          )}
                          {fkError && <p className="mt-2 select-text text-xs text-destructive-foreground">{fkError}</p>}
                        </div>
                      );
                    })}
                  </div>
                )}
                {!capabilities.schemas && (
                  <p className="mt-2 text-xs text-muted-foreground">
                    <Rich id="ui.createTablePage.sameSchema" params={{ schema: schema || initialSchema }} tags={{ schema: (text) => <span className="font-mono">{text}</span> }} />
                  </p>
                )}
              </section>

              {data.createTableFlags.length > 0 && (
                <section className="flex flex-col gap-2">
                  <h2 className="text-sm font-semibold">{t("ui.createTablePage.options")}</h2>
                  {(hasFlag("withoutRowid") || hasFlag("strict")) && (
                    <div className="flex flex-wrap gap-x-6 gap-y-1">
                      {hasFlag("withoutRowid") && <CheckOption checked={withoutRowid} onChange={setWithoutRowid} label="WITHOUT ROWID" />}
                      {hasFlag("strict") && <CheckOption checked={strict} onChange={setStrict} label="STRICT" />}
                    </div>
                  )}
                  {hasFlag("unlogged") && <CheckOption checked={unlogged} onChange={setUnlogged} label="UNLOGGED" />}
                  {hasFlag("partition") && (
                    <>
                      <div className="grid grid-cols-[9rem_1fr] gap-2">
                        <select aria-label={t("ui.createTablePage.partitionMethod")} value={partitionMethod} onChange={(e) => setPartitionMethod(e.target.value)} className={cn(selectClass, "h-8 w-full")}>
                          <option value="">{t("ui.createTablePage.noPartition")}</option>
                          {PARTITION_METHODS.map((method) => (
                            <option key={method} value={method}>
                              PARTITION BY {method}
                            </option>
                          ))}
                        </select>
                        <Input
                          value={partitionExpr}
                          onChange={(e) => setPartitionExpr(e.target.value)}
                          aria-label={t("ui.createTablePage.partitionKey")}
                          placeholder={t("ui.createTablePage.partitionKeyPlaceholder")}
                          disabled={!partitionMethod}
                          spellCheck={false}
                          className={cn("h-8 font-mono text-xs", partitionIncomplete && "border-destructive")}
                        />
                      </div>
                      {partitionIncomplete && <p className="select-text text-xs text-destructive-foreground">{t("ui.createTablePage.partitionKeyRequired")}</p>}
                    </>
                  )}
                </section>
              )}

              <SqlPreviewBox
                source={sql}
                error={previewError}
                onOpen={() => {
                  if (!sql) return;
                  requestOpenQuery(tabId, sql);
                }}
              />
            </>
          )}
        </div>
      </div>

      <div className="flex h-12 shrink-0 items-center gap-3 border-t border-border px-6">
        {mode === "simple" && (hasIncompleteForeignKey || partitionIncomplete) ? (
          <p className="min-w-0 flex-1 text-xs text-muted-foreground">{t("ui.createTablePage.finishAdvanced")}</p>
        ) : null}
        <Button variant="primary" size="sm" className="ml-auto" onClick={create} disabled={!sql || busy || hasIncompleteForeignKey || partitionIncomplete || !table.trim() || !schema.trim()}>
          {t("ui.createTablePage.createTable")}
        </Button>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex w-full items-center gap-3">
      <span className="w-24 shrink-0 text-[13px] text-muted-foreground">{label}</span>
      <span className="min-w-0 flex-1">{children}</span>
    </label>
  );
}
