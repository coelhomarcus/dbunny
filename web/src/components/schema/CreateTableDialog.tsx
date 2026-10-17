import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Plus, Table2, X } from "lucide-react";
import type { ColumnGeneration, CompletionRelation, NewColumn, NewTable } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useTab } from "../../contexts/workspace";
import { foreignKeyDraftError, foreignKeysForSpec } from "../../lib/foreignKeyForm";
import ForeignKeyEditor from "../structure/ForeignKeyEditor";
import { requestOpenQuery } from "../../lib/openQuery";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import CheckOption from "../ui/CheckOption";
import { selectClass } from "../ui/styles";
import { SqlPreviewBox } from "./SqlPreview";

const COMMON_TYPES = ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
// SQLite stores a few kinds of value; the type decides how a column converts what is written to it
const SQLITE_TYPES = ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"];
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

type Row = NewColumn & { key: number };

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
  const nextKey = useRef(0);
  const newRow = (patch: Partial<NewColumn> = {}): Row => ({
    key: nextKey.current++,
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
  const [table, setTable] = useState("");
  const [rows, setRows] = useState<Row[]>(() =>
    dialect === "sqlite"
      ? [
          // A lone INTEGER primary key is the row id: SQLite numbers new rows by itself
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
    const firstFkError = fkErrors.find(Boolean);
    if (firstFkError) {
      setError(firstFkError);
      return;
    }
    if (partitionIncomplete) {
      setError("Enter a partition key");
      return;
    }
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
      className="max-w-3xl"
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={create} disabled={!sql || busy || hasIncompleteForeignKey || partitionIncomplete}>
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
          <Button variant="ghost-muted" size="xs" className="self-start" onClick={() => setRows((prev) => [...prev, newRow(dialect === "sqlite" ? { dataType: "TEXT" } : {})])}>
            <Plus /> Add column
          </Button>
        </div>

        <div className="flex flex-col gap-2">
          {foreignKeys.map((fk, index) => {
            const fkError = fkErrors[index];
            const patch = (next: Partial<ForeignKey>) => setForeignKeys((prev) => prev.map((item) => (item.key === fk.key ? { ...item, ...next } : item)));
            return (
              <div key={fk.key} className={cn("flex flex-col gap-2 rounded-lg border p-2.5", fkError ? "border-destructive" : "border-border")}>
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[13px] font-medium">Foreign key</span>
                  <Button variant="ghost-muted" size="icon-xs" aria-label="Remove foreign key" onClick={() => setForeignKeys((prev) => prev.filter((item) => item.key !== fk.key))}>
                    <X />
                  </Button>
                </div>
                <ForeignKeyEditor
                  relations={relations}
                  value={fk}
                  onChange={patch}
                  localColumnNames={named}
                  selectableLocalColumns
                  error={fkError}
                />
              </div>
            );
          })}
          <Button
            variant="ghost-muted"
            size="xs"
            className="self-start"
            onClick={() =>
              setForeignKeys((prev) => [
                ...prev,
                { key: nextKey.current++, columns: [], refTable: "", refColumns: {}, onUpdate: "NO ACTION", onDelete: "NO ACTION" },
              ])
            }
          >
            <Plus /> Add foreign key
          </Button>
          {sqlite && (
            <p className="text-xs text-muted-foreground">
              Referenced tables must be in schema <span className="font-mono">{schema}</span> (same database).
            </p>
          )}
        </div>

        {sqlite ? (
          <div className="flex flex-wrap gap-x-4 gap-y-1">
            <CheckOption checked={withoutRowid} onChange={setWithoutRowid} label="WITHOUT ROWID" />
            <CheckOption checked={strict} onChange={setStrict} label="STRICT" />
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <CheckOption checked={unlogged} onChange={setUnlogged} label="UNLOGGED" />
            <div className="flex flex-col gap-1">
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
