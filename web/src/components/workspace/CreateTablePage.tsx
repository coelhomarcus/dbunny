import { useEffect, useRef, useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import type { ColumnGeneration, CompletionRelation, NewColumn, NewTable } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useTab } from "../../contexts/workspace";
import { foreignKeyDraftError, foreignKeysForSpec, guessReference, REFERENTIAL_ACTIONS, type ForeignKeyDraft } from "../../lib/foreignKeyForm";
import { requestSchemaRefresh } from "../../lib/schemaRefresh";
import { requestOpenQuery } from "../../lib/openQuery";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Input from "../ui/Input";
import CheckOption from "../ui/CheckOption";
import Checkbox from "../Checkbox";
import SegmentedTabs from "../ui/SegmentedTabs";
import { selectClass } from "../ui/styles";
import { SqlPreviewBox } from "../schema/SqlPreview";

const COMMON_TYPES = ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
const SQLITE_TYPES = ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"];
const PARTITION_METHODS = ["RANGE", "LIST", "HASH"];

type ForeignKey = ForeignKeyDraft & { key: number };
type Row = NewColumn & { key: number };
type Mode = "simple" | "advanced";

const cellClass =
  "h-9 border-0 bg-transparent px-2.5 shadow-none focus-visible:border-transparent focus-visible:ring-0 dark:bg-transparent";

/** A full page for a new table: a short column list, or the full set of options. */
export default function CreateTablePage({ documentId, schema: initialSchema }: { documentId: string; schema: string }) {
  const api = useApi();
  const { tabId } = useTab();
  const { openDocument, closeDocument, updateCreateTable } = useDocuments();
  const dialect = useSqlDialect();
  const capabilities = useCapabilities();
  const sqlite = dialect === "sqlite";
  const suggestedTypes = sqlite ? SQLITE_TYPES : COMMON_TYPES;
  const nextKey = useRef(0);
  const newRow = (patch: Partial<NewColumn> = {}): Row => ({
    key: nextKey.current++,
    name: "",
    dataType: sqlite ? "TEXT" : "text",
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
    sqlite
      ? [
          newRow({ name: "id", dataType: "INTEGER", nullable: false, primaryKey: true }),
          newRow({ name: "created_at", dataType: "TEXT", nullable: false, default: "CURRENT_TIMESTAMP" }),
        ]
      : [
          newRow({ name: "id", dataType: "bigserial", nullable: false, primaryKey: true }),
          newRow({ name: "created_at", dataType: "timestamptz", nullable: false, default: "now()" }),
        ],
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
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    updateCreateTable(documentId, { title: table, schema });
  }, [documentId, table, schema, updateCreateTable]);

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
    generated: r.generation === "generated" && r.generated?.trim() ? r.generated : null,
    default: r.generation === "none" && r.default?.trim() ? r.default : null,
  }));
  const named = columns.map((c) => c.name.trim()).filter(Boolean);
  const fkErrors = foreignKeys.map((fk) => foreignKeyDraftError(fk, named));
  const hasIncompleteForeignKey = fkErrors.some(Boolean);
  const partitionKey = partitionExpr.trim();
  const partitionIncomplete = !sqlite && !!partitionMethod && !partitionKey;
  const spec: NewTable = {
    columns,
    foreignKeys: foreignKeysForSpec(foreignKeys, relations, named),
    unlogged: !sqlite && unlogged,
    withoutRowid: sqlite && withoutRowid,
    strict: sqlite && strict,
    partition: !sqlite && partitionMethod && partitionKey ? { method: partitionMethod, expression: partitionKey } : null,
  };
  const specKey = JSON.stringify(spec);

  useEffect(() => {
    if (!schema.trim()) return;
    let stale = false;
    const timer = setTimeout(() => {
      api
        .previewCreateTable(schema.trim(), table, JSON.parse(specKey) as NewTable)
        .then((text) => !stale && (setSql(text), setPreviewError(null)))
        .catch((err) => !stale && (setSql(""), setPreviewError(typeof err === "string" ? err : "Invalid table")));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, specKey]);

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
    const firstFkError = fkErrors.find(Boolean);
    if (firstFkError) {
      setError(firstFkError);
      return;
    }
    if (partitionIncomplete) {
      setError("Enter a partition key");
      return;
    }
    const name = table.trim();
    const schemaName = schema.trim();
    if (!name || !schemaName) return;
    setBusy(true);
    setError(null);
    try {
      await api.createTable(schemaName, name, spec);
      requestSchemaRefresh(tabId, schemaName);
      openDocument({ kind: "data", schema: schemaName, name, icon: "table" });
      closeDocument(documentId);
    } catch (err) {
      setError(typeof err === "string" ? err : "Couldn't create the table");
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
        <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
          <div className="flex flex-col gap-2">
            <Field label="Schema">
              <Input
                value={schema}
                onChange={(e) => setSchema(e.target.value)}
                readOnly={!capabilities.schemas}
                spellCheck={false}
                aria-label="Schema"
                className="h-9 border-0 bg-foreground/[0.04] font-medium shadow-none focus-visible:ring-1 dark:bg-foreground/[0.04]"
              />
            </Field>
            <Field label="Table name">
              <Input
                value={table}
                onChange={(e) => setTable(e.target.value)}
                placeholder="untitled_table"
                autoFocus
                spellCheck={false}
                aria-label="Table name"
                className="h-9 border-0 bg-foreground/[0.04] font-medium shadow-none focus-visible:ring-1 dark:bg-foreground/[0.04]"
              />
            </Field>
          </div>

          <div>
            <div className="mb-3 flex items-center gap-3">
              <h2 className="text-sm font-semibold">Columns</h2>
              <SegmentedTabs
                value={mode}
                onChange={setMode}
                options={[
                  { value: "simple", label: "Simple" },
                  { value: "advanced", label: "Advanced" },
                ]}
              />
              <Button
                variant="primary"
                size="icon-sm"
                aria-label="Add column"
                className="ml-auto rounded-full"
                onClick={() => setRows((prev) => [...prev, newRow()])}
              >
                <Plus />
              </Button>
            </div>

            <div className={advanced ? "overflow-x-auto" : undefined}>
            <div className={advanced ? "min-w-[64rem]" : undefined}>
            <div className={cn("grid items-center px-1 pb-1 text-[11px] text-muted-foreground", grid)}>
              <span className="px-2.5">Name</span>
              <span className="px-2.5">Type</span>
              <span className="text-center">Nullable</span>
              <span className="px-2.5">Default value</span>
              {capabilities.comments && <span className="px-2.5">Comment</span>}
              <span className="text-center">Primary</span>
              {advanced && <span className="text-center">Unique</span>}
              {advanced && <span className="px-2.5">Generation</span>}
              {advanced && <span className="px-2.5">Check</span>}
              <span />
            </div>

            <div className="flex flex-col gap-1">
              {rows.map((r) => (
                <div key={r.key} className="rounded-md bg-foreground/[0.045]">
                  <div className={cn("grid items-center", grid)}>
                    <Input value={r.name} onChange={(e) => update(r.key, { name: e.target.value })} aria-label="Column name" placeholder="name" spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    <Input value={r.dataType} onChange={(e) => update(r.key, { dataType: e.target.value })} aria-label="Type" spellCheck={false} list={`dbunny-new-table-types-${documentId}`} className={cn(cellClass, "font-mono text-[13px]")} />
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
                      aria-label="Default value"
                      placeholder="(NULL)"
                      disabled={r.generation !== "none"}
                      spellCheck={false}
                      className={cn(cellClass, "font-mono text-[13px]")}
                    />
                    {capabilities.comments && (
                      <Input value={r.comment ?? ""} onChange={(e) => update(r.key, { comment: e.target.value })} aria-label="Comment" placeholder="(NULL)" className={cn(cellClass, "text-[13px]")} />
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
                        aria-label="Generation"
                        value={r.generation}
                        onChange={(e) => update(r.key, { generation: e.target.value as ColumnGeneration })}
                        className="h-9 w-full cursor-pointer bg-transparent px-2 text-[13px] outline-none"
                      >
                        <option value="none">None</option>
                        {sqlite ? (
                          <option value="autoincrement">Autoincrement</option>
                        ) : (
                          <>
                            <option value="identityByDefault">Identity by default</option>
                            <option value="identityAlways">Identity always</option>
                          </>
                        )}
                        <option value="generated">Generated</option>
                      </select>
                    )}
                    {advanced && (
                      <Input value={r.check ?? ""} onChange={(e) => update(r.key, { check: e.target.value })} aria-label="Check" placeholder="price >= 0" spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    )}
                    <span className="flex justify-center">
                      <Button variant="ghost-muted" size="icon-xs" aria-label={`Remove ${r.name || "column"}`} onClick={() => setRows((prev) => prev.filter((p) => p.key !== r.key))} disabled={rows.length === 1}>
                        <X />
                      </Button>
                    </span>
                  </div>
                  {advanced && r.generation === "generated" && (
                    <div className="grid grid-cols-[7.5rem_1fr] items-center border-t border-border/50">
                      <span className="px-2.5 text-[11px] text-muted-foreground">Expression</span>
                      <Input value={r.generated ?? ""} onChange={(e) => update(r.key, { generated: e.target.value })} aria-label="Generated expression" placeholder="e.g. price * 2" spellCheck={false} className={cn(cellClass, "font-mono text-[13px]")} />
                    </div>
                  )}
                </div>
              ))}
            </div>
            </div>
            </div>
            <datalist id={`dbunny-new-table-types-${documentId}`}>
              {suggestedTypes.map((t) => (
                <option key={t} value={t} />
              ))}
            </datalist>
          </div>

          {advanced && (
            <>
              <section>
                <div className="mb-3 flex items-center">
                  <h2 className="text-sm font-semibold">Foreign keys</h2>
                  <Button
                    variant="primary"
                    size="icon-sm"
                    aria-label="Add foreign key"
                    className="ml-auto rounded-full"
                    onClick={() =>
                      setForeignKeys((prev) => [
                        ...prev,
                        { key: nextKey.current++, columns: [], refTable: "", refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" },
                      ])
                    }
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
                                <span className="text-[13px] text-muted-foreground">Name a column first.</span>
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
                            <Button variant="ghost-muted" size="icon-xs" aria-label="Remove foreign key" onClick={() => setForeignKeys((prev) => prev.filter((item) => item.key !== fk.key))}>
                              <X />
                            </Button>
                          </div>
                          <div className="grid gap-2 sm:grid-cols-3">
                            <select
                              aria-label="References"
                              value={fk.refTable}
                              onChange={(e) => patch({ refTable: e.target.value, refColumns: {} })}
                              className={cn(selectClass, "w-full")}
                            >
                              <option value="">{relations === null ? "Loading…" : "References"}</option>
                              {(relations ?? []).map((r) => (
                                <option key={`${r.schema}.${r.name}`} value={`${r.schema}.${r.name}`}>
                                  {r.schema}.{r.name}
                                </option>
                              ))}
                            </select>
                            <select aria-label="On delete" value={fk.onDelete} onChange={(e) => patch({ onDelete: e.target.value })} className={cn(selectClass, "w-full")}>
                              {REFERENTIAL_ACTIONS.map((action) => (
                                <option key={action} value={action}>
                                  On delete {action.toLowerCase()}
                                </option>
                              ))}
                            </select>
                            <select aria-label="On update" value={fk.onUpdate} onChange={(e) => patch({ onUpdate: e.target.value })} className={cn(selectClass, "w-full")}>
                              {REFERENTIAL_ACTIONS.map((action) => (
                                <option key={action} value={action}>
                                  On update {action.toLowerCase()}
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
                                    aria-label={`Column referenced by ${column}`}
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
                          {fkError && <p className="mt-2 text-xs text-destructive-foreground">{fkError}</p>}
                        </div>
                      );
                    })}
                  </div>
                )}
                {sqlite && (
                  <p className="mt-2 text-xs text-muted-foreground">
                    Referenced tables must be in schema <span className="font-mono">{schema || initialSchema}</span> (same database).
                  </p>
                )}
              </section>

              <section className="flex flex-col gap-2">
                <h2 className="text-sm font-semibold">Options</h2>
                {sqlite ? (
                  <div className="flex flex-wrap gap-x-6 gap-y-1">
                    <CheckOption checked={withoutRowid} onChange={setWithoutRowid} label="WITHOUT ROWID" />
                    <CheckOption checked={strict} onChange={setStrict} label="STRICT" />
                  </div>
                ) : (
                  <>
                    <CheckOption checked={unlogged} onChange={setUnlogged} label="UNLOGGED" />
                    <div className="grid grid-cols-[9rem_1fr] gap-2">
                      <select aria-label="Partition method" value={partitionMethod} onChange={(e) => setPartitionMethod(e.target.value)} className={cn(selectClass, "h-8 w-full")}>
                        <option value="">No partition</option>
                        {PARTITION_METHODS.map((method) => (
                          <option key={method} value={method}>
                            PARTITION BY {method}
                          </option>
                        ))}
                      </select>
                      <Input
                        value={partitionExpr}
                        onChange={(e) => setPartitionExpr(e.target.value)}
                        aria-label="Partition key"
                        placeholder="partition key, e.g. created_at"
                        disabled={!partitionMethod}
                        spellCheck={false}
                        className={cn("h-8 font-mono text-xs", partitionIncomplete && "border-destructive")}
                      />
                    </div>
                    {partitionIncomplete && <p className="text-xs text-destructive-foreground">Enter a partition key</p>}
                  </>
                )}
              </section>

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
        {error ? (
          <p className="min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>
        ) : mode === "simple" && (hasIncompleteForeignKey || partitionIncomplete) ? (
          <p className="min-w-0 flex-1 text-xs text-muted-foreground">Finish the advanced options before creating.</p>
        ) : null}
        <Button variant="primary" size="sm" className="ml-auto" onClick={create} disabled={!sql || busy || hasIncompleteForeignKey || partitionIncomplete || !table.trim() || !schema.trim()}>
          Create table
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
