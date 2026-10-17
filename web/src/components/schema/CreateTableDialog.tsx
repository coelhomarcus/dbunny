import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { ArrowRight, Plus, Table2, X } from "lucide-react";
import type { ColumnGeneration, CompletionRelation, NewColumn, NewTable } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useTab } from "../../contexts/workspace";
import { requestOpenQuery } from "../../lib/openQuery";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import CheckOption from "../ui/CheckOption";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";
import { SqlPreviewBox } from "./SqlPreview";

const COMMON_TYPES = ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
// SQLite stores a few kinds of value; the type decides how a column converts what is written to it
const SQLITE_TYPES = ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"];
const REFERENTIAL_ACTIONS = ["NO ACTION", "RESTRICT", "CASCADE", "SET NULL", "SET DEFAULT"];
const PARTITION_METHODS = ["RANGE", "LIST", "HASH"];

type ForeignKey = {
  key: number;
  columns: string[];
  /** `schema.name` of the referenced table, or empty while it is still being chosen. */
  refTable: string;
  refColumns: Record<string, string>;
  onUpdate: string;
  onDelete: string;
};

let nextKey = 0;

/** The referenced column a new foreign key column most likely points at. */
function guessReference(column: string, target: CompletionRelation | undefined): string {
  const names = target?.columns.map((c) => c.name) ?? [];
  return names.find((n) => n === column) ?? names.find((n) => n === "id") ?? names[0] ?? "";
}

type Row = NewColumn & { key: number };

const row = (patch: Partial<NewColumn> = {}): Row => ({
  key: nextKey++,
  name: "",
  dataType: "text",
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

function Builder({
  schema,
  onCreated,
  onClose,
}: {
  schema: string;
  onCreated: (table: string) => void;
  onClose: () => void;
}) {
  const api = useApi();
  const navigate = useNavigate();
  const { tabId } = useTab();
  const dialect = useSqlDialect();
  const capabilities = useCapabilities();
  const suggestedTypes = dialect === "sqlite" ? SQLITE_TYPES : COMMON_TYPES;
  const [table, setTable] = useState("");
  const [rows, setRows] = useState<Row[]>(() =>
    dialect === "sqlite"
      ? [
          // A lone INTEGER primary key is the row id: SQLite numbers new rows by itself
          row({ name: "id", dataType: "INTEGER", nullable: false, primaryKey: true }),
          row({ name: "created_at", dataType: "TEXT", nullable: false, default: "CURRENT_TIMESTAMP" }),
        ]
      : [
          row({ name: "id", dataType: "bigserial", nullable: false, primaryKey: true }),
          row({ name: "created_at", dataType: "timestamptz", nullable: false, default: "now()" }),
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
  const sqlite = dialect === "sqlite";

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
  const spec: NewTable = {
    columns,
    foreignKeys: foreignKeys.flatMap((fk) => {
      const target = relations?.find((r) => `${r.schema}.${r.name}` === fk.refTable);
      if (!target) return [];
      const locals = fk.columns.filter((c) => named.includes(c));
      return [
        {
          columns: locals,
          refSchema: target.schema,
          refTable: target.name,
          refColumns: locals.map((c) => fk.refColumns[c] ?? guessReference(c, target)),
          onUpdate: fk.onUpdate,
          onDelete: fk.onDelete,
        },
      ];
    }),
    unlogged: !sqlite && unlogged,
    withoutRowid: sqlite && withoutRowid,
    strict: sqlite && strict,
    partition: !sqlite && partitionMethod ? { method: partitionMethod, expression: partitionExpr } : null,
  };
  const specKey = JSON.stringify(spec);

  useEffect(() => {
    let stale = false;
    const timer = setTimeout(() => {
      api
        .previewCreateTable(schema, table, JSON.parse(specKey))
        .then((text) => !stale && (setSql(text), setPreviewError(null)))
        .catch((err) => !stale && (setSql(""), setPreviewError(typeof err === "string" ? err : "Invalid table")));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, specKey, capabilities.comments]);

  const update = (key: number, patch: Partial<NewColumn>) => setRows((prev) => prev.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await api.createTable(schema, table.trim(), spec);
      onCreated(table.trim());
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : "Couldn't create the table");
      setBusy(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      icon={<Table2 />}
      title={
        <>
          New table <span className="font-normal text-muted-foreground">in {schema}</span>
        </>
      }
      className="max-w-2xl"
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={create} disabled={!sql || busy}>
            Create table
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        <label className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium">Name</span>
          <Input value={table} onChange={(e) => setTable(e.target.value)} placeholder="e.g. invoices" autoFocus spellCheck={false} className="font-mono" />
        </label>

        <div className="flex flex-col gap-2">
          {rows.map((r) => (
            <div key={r.key} className="flex flex-col gap-2 rounded-lg border border-border p-2.5">
              <div className="flex items-center gap-2">
                <Input value={r.name} onChange={(e) => update(r.key, { name: e.target.value })} aria-label="Column name" placeholder="name" spellCheck={false} className="h-8 min-w-0 flex-1 font-mono text-xs" />
                <Input value={r.dataType} onChange={(e) => update(r.key, { dataType: e.target.value })} aria-label="Type" spellCheck={false} list="dbunny-new-table-types" className="h-8 w-40 shrink-0 font-mono text-xs" />
                <Button variant="ghost-muted" size="icon-xs" aria-label={`Remove ${r.name || "column"}`} onClick={() => setRows((prev) => prev.filter((p) => p.key !== r.key))} disabled={rows.length === 1}>
                  <X />
                </Button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <select
                  aria-label="Generation"
                  value={r.generation}
                  onChange={(e) => update(r.key, { generation: e.target.value as ColumnGeneration })}
                  className={cn(selectClass, "h-8 font-mono text-xs")}
                >
                  <option value="none">No generation</option>
                  {dialect === "sqlite" ? (
                    <option value="autoincrement">Autoincrement</option>
                  ) : (
                    <>
                      <option value="identityByDefault">Identity, by default</option>
                      <option value="identityAlways">Identity, always</option>
                    </>
                  )}
                  <option value="generated">Generated</option>
                </select>
                {r.generation === "generated" && (
                  <Input value={r.generated ?? ""} onChange={(e) => update(r.key, { generated: e.target.value })} aria-label="Generated expression" placeholder="e.g. price * 2" spellCheck={false} className="h-8 min-w-0 flex-1 font-mono text-xs" />
                )}
              </div>
              <div className="flex flex-wrap gap-x-4 gap-y-1">
                <CheckOption checked={!r.nullable || r.primaryKey} onChange={() => !r.primaryKey && update(r.key, { nullable: !r.nullable })} label="NOT NULL" />
                <CheckOption checked={r.primaryKey} onChange={() => update(r.key, { primaryKey: !r.primaryKey })} label="Primary key" />
                <CheckOption checked={r.unique} onChange={() => update(r.key, { unique: !r.unique })} label="Unique" />
              </div>
              <div className={capabilities.comments ? "grid grid-cols-3 gap-2" : "grid grid-cols-2 gap-2"}>
                <Input value={r.generation === "none" ? (r.default ?? "") : ""} onChange={(e) => update(r.key, { default: e.target.value })} aria-label="Default" placeholder="default" disabled={r.generation !== "none"} spellCheck={false} className="h-8 font-mono text-xs" />
                <Input value={r.check ?? ""} onChange={(e) => update(r.key, { check: e.target.value })} aria-label="Check" placeholder="check, e.g. price >= 0" spellCheck={false} className="h-8 font-mono text-xs" />
                {capabilities.comments && <Input value={r.comment ?? ""} onChange={(e) => update(r.key, { comment: e.target.value })} aria-label="Comment" placeholder="comment" className="h-8 text-xs" />}
              </div>
            </div>
          ))}
          <datalist id="dbunny-new-table-types">
            {suggestedTypes.map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
          <Button variant="ghost-muted" size="xs" className="self-start" onClick={() => setRows((prev) => [...prev, row(dialect === "sqlite" ? { dataType: "TEXT" } : {})])}>
            <Plus /> Add column
          </Button>
        </div>

        <div className="flex flex-col gap-2">
          {foreignKeys.map((fk) => {
            const target = relations?.find((r) => `${r.schema}.${r.name}` === fk.refTable);
            const locals = fk.columns.filter((c) => named.includes(c));
            const patch = (next: Partial<ForeignKey>) => setForeignKeys((prev) => prev.map((item) => (item.key === fk.key ? { ...item, ...next } : item)));
            return (
              <div key={fk.key} className="flex flex-col gap-2 rounded-lg border border-border p-2.5">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[13px] font-medium">Foreign key</span>
                  <Button variant="ghost-muted" size="icon-xs" aria-label="Remove foreign key" onClick={() => setForeignKeys((prev) => prev.filter((item) => item.key !== fk.key))}>
                    <X />
                  </Button>
                </div>
                <div className="flex flex-col gap-1">
                  <span className="text-[13px] font-medium">Columns</span>
                  {named.length === 0 ? (
                    <p className="text-xs text-muted-foreground">Name a column first.</p>
                  ) : (
                    <div className="flex flex-col gap-1">
                      {named.map((name) => {
                        const on = fk.columns.includes(name);
                        return (
                          <div
                            key={name}
                            className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-accent"
                            onClick={() => patch({ columns: on ? fk.columns.filter((c) => c !== name) : [...fk.columns, name] })}
                          >
                            <Checkbox checked={on} onChange={() => patch({ columns: on ? fk.columns.filter((c) => c !== name) : [...fk.columns, name] })} />
                            <span className="font-mono text-xs">{name}</span>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
                <label className="flex flex-col gap-1.5">
                  <span className="text-[13px] font-medium">References</span>
                  <select value={fk.refTable} onChange={(e) => patch({ refTable: e.target.value, refColumns: {} })} className={cn(selectClass, "w-full")}>
                    <option value="">{relations === null ? "Loading…" : "Choose a table"}</option>
                    {(relations ?? []).map((r) => (
                      <option key={`${r.schema}.${r.name}`} value={`${r.schema}.${r.name}`}>
                        {r.schema}.{r.name}
                      </option>
                    ))}
                  </select>
                </label>
                {target && locals.length > 0 && (
                  <div className="flex flex-col gap-1.5">
                    {locals.map((c) => (
                      <div key={c} className="grid grid-cols-[1fr_auto_1fr] items-center gap-2">
                        <span className="truncate font-mono text-xs">{c}</span>
                        <ArrowRight size={13} className="text-muted-foreground" />
                        <select
                          value={fk.refColumns[c] ?? guessReference(c, target)}
                          onChange={(e) => patch({ refColumns: { ...fk.refColumns, [c]: e.target.value } })}
                          className={cn(selectClass, "w-full font-mono text-xs")}
                          aria-label={`Column referenced by ${c}`}
                        >
                          {target.columns.map((rc) => (
                            <option key={rc.name} value={rc.name}>
                              {rc.name}
                            </option>
                          ))}
                        </select>
                      </div>
                    ))}
                  </div>
                )}
                <div className="grid grid-cols-2 gap-2">
                  <label className="flex flex-col gap-1.5">
                    <span className="text-[13px] font-medium">On delete</span>
                    <select value={fk.onDelete} onChange={(e) => patch({ onDelete: e.target.value })} className={cn(selectClass, "w-full")}>
                      {REFERENTIAL_ACTIONS.map((a) => (
                        <option key={a} value={a}>
                          {a.toLowerCase()}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex flex-col gap-1.5">
                    <span className="text-[13px] font-medium">On update</span>
                    <select value={fk.onUpdate} onChange={(e) => patch({ onUpdate: e.target.value })} className={cn(selectClass, "w-full")}>
                      {REFERENTIAL_ACTIONS.map((a) => (
                        <option key={a} value={a}>
                          {a.toLowerCase()}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              </div>
            );
          })}
          <Button
            variant="ghost-muted"
            size="xs"
            className="self-start"
            onClick={() => setForeignKeys((prev) => [...prev, { key: nextKey++, columns: [], refTable: "", refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" }])}
          >
            <Plus /> Add foreign key
          </Button>
        </div>

        {sqlite ? (
          <div className="flex flex-wrap gap-x-4 gap-y-1">
            <CheckOption checked={withoutRowid} onChange={setWithoutRowid} label="WITHOUT ROWID" />
            <CheckOption checked={strict} onChange={setStrict} label="STRICT" />
          </div>
        ) : (
          <div className="flex flex-col gap-2">
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
                className="h-8 font-mono text-xs"
              />
            </div>
          </div>
        )}

        <SqlPreviewBox
          source={sql}
          error={previewError}
          onOpen={() => {
            if (!sql) return;
            requestOpenQuery(tabId, sql);
            onClose();
            navigate("/db/query");
          }}
        />
      </div>
    </Dialog>
  );
}

/** Builds a CREATE TABLE from a list of columns. Mounted only while open. */
export default function CreateTableDialog({ schema, ...props }: { schema: string | null } & Omit<Parameters<typeof Builder>[0], "schema">) {
  return schema ? <Builder schema={schema} {...props} /> : null;
}
