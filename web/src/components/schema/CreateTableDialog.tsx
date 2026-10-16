import { useEffect, useState } from "react";
import { Plus, Table2, X } from "lucide-react";
import type { NewColumn } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";
import SqlPreview from "./SqlPreview";

const COMMON_TYPES = ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"];
// SQLite stores a few kinds of value; the type decides how a column converts what is written to it
const SQLITE_TYPES = ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"];

type Row = NewColumn & { key: number };

let nextKey = 0;
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
  const [sql, setSql] = useState("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const columns: NewColumn[] = rows.map((r) => ({
    name: r.name,
    dataType: r.dataType,
    nullable: r.nullable,
    primaryKey: r.primaryKey,
    default: r.default?.trim() ? r.default : null,
    unique: r.unique,
    check: r.check?.trim() ? r.check : null,
    comment: capabilities.comments && r.comment?.trim() ? r.comment : null,
  }));
  const columnsKey = JSON.stringify(columns);

  useEffect(() => {
    let stale = false;
    const timer = setTimeout(() => {
      api
        .previewCreateTable(schema, table, JSON.parse(columnsKey))
        .then((text) => !stale && (setSql(text), setPreviewError(null)))
        .catch((err) => !stale && (setSql(""), setPreviewError(typeof err === "string" ? err : "Invalid table")));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [api, schema, table, columnsKey, capabilities.comments]);

  const update = (key: number, patch: Partial<NewColumn>) => setRows((prev) => prev.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await api.createTable(schema, table.trim(), columns);
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
      className="max-w-5xl"
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

        <div className="overflow-hidden rounded-lg border border-border">
          <table className="w-full text-[13px]">
            <thead className="bg-card-header text-xs text-muted-foreground">
              <tr className="[&>th]:px-2 [&>th]:py-1.5 [&>th]:text-left [&>th]:font-medium">
                <th>Column</th>
                <th>Type</th>
                <th className="w-px whitespace-nowrap">NOT NULL</th>
                <th className="w-px whitespace-nowrap">Primary key</th>
                <th className="w-px whitespace-nowrap">Unique</th>
                <th>Default</th>
                <th>Check</th>
                {capabilities.comments && <th>Comment</th>}
                <th className="w-px" />
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.key} className="border-t border-border [&>td]:px-2 [&>td]:py-1">
                  <td>
                    <Input value={r.name} onChange={(e) => update(r.key, { name: e.target.value })} aria-label="Column name" spellCheck={false} className="h-7 font-mono text-xs" />
                  </td>
                  <td>
                    <select
                      value={r.dataType}
                      onChange={(e) => update(r.key, { dataType: e.target.value })}
                      aria-label="Type"
                      className={cn(selectClass, "h-7 w-full min-w-28 font-mono text-xs")}
                    >
                      {(suggestedTypes.includes(r.dataType) ? suggestedTypes : [r.dataType, ...suggestedTypes]).map((t) => (
                        <option key={t} value={t}>
                          {t}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="text-center">
                    <Checkbox checked={!r.nullable || r.primaryKey} onChange={() => update(r.key, { nullable: !r.nullable })} className={cn(r.primaryKey && "opacity-50")} />
                  </td>
                  <td className="text-center">
                    <Checkbox checked={r.primaryKey} onChange={() => update(r.key, { primaryKey: !r.primaryKey })} />
                  </td>
                  <td className="text-center">
                    <Checkbox checked={r.unique} onChange={() => update(r.key, { unique: !r.unique })} />
                  </td>
                  <td>
                    <Input value={r.default ?? ""} onChange={(e) => update(r.key, { default: e.target.value })} aria-label="Default" placeholder="none" spellCheck={false} className="h-7 font-mono text-xs" />
                  </td>
                  <td>
                    <Input value={r.check ?? ""} onChange={(e) => update(r.key, { check: e.target.value })} aria-label="Check" placeholder="e.g. price >= 0" spellCheck={false} className="h-7 font-mono text-xs" />
                  </td>
                  {capabilities.comments && (
                    <td>
                      <Input value={r.comment ?? ""} onChange={(e) => update(r.key, { comment: e.target.value })} aria-label="Comment" placeholder="optional" className="h-7 text-xs" />
                    </td>
                  )}
                  <td>
                    <Button variant="ghost-muted" size="icon-xs" aria-label={`Remove ${r.name || "column"}`} onClick={() => setRows((prev) => prev.filter((p) => p.key !== r.key))} disabled={rows.length === 1}>
                      <X />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="border-t border-border p-1">
            <Button variant="ghost-muted" size="xs" onClick={() => setRows((prev) => [...prev, row(dialect === "sqlite" ? { dataType: "TEXT" } : {})])}>
              <Plus /> Add column
            </Button>
          </div>
        </div>

        <div>
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">SQL</span>
          {sql && !previewError ? (
            <div className="mt-1 max-h-40 overflow-auto rounded-lg border border-border bg-muted px-3 py-2 font-mono text-xs">
              <SqlPreview source={sql} />
            </div>
          ) : (
            <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-muted px-3 py-2 font-mono text-xs text-muted-foreground">
              {previewError || "…"}
            </pre>
          )}
        </div>
      </div>
    </Dialog>
  );
}

/** Builds a CREATE TABLE from a list of columns. Mounted only while open. */
export default function CreateTableDialog({ schema, ...props }: { schema: string | null } & Omit<Parameters<typeof Builder>[0], "schema">) {
  return schema ? <Builder schema={schema} {...props} /> : null;
}
