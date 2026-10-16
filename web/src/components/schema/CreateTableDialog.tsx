import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { Plus, Table2, X } from "lucide-react";
import type { NewColumn } from "@/types";
import { useApi } from "../../contexts/useConnection";
import { useTab } from "../../contexts/workspace";
import { requestOpenQuery } from "../../lib/openQuery";
import { useCapabilities } from "../../hooks/useCapabilities";
import { useSqlDialect } from "../../hooks/useSqlDialect";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import CheckOption from "../ui/CheckOption";
import { SqlPreviewBox } from "./SqlPreview";

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
              <div className="flex flex-wrap gap-x-4 gap-y-1">
                <CheckOption checked={!r.nullable || r.primaryKey} onChange={() => !r.primaryKey && update(r.key, { nullable: !r.nullable })} label="NOT NULL" />
                <CheckOption checked={r.primaryKey} onChange={() => update(r.key, { primaryKey: !r.primaryKey })} label="Primary key" />
                <CheckOption checked={r.unique} onChange={() => update(r.key, { unique: !r.unique })} label="Unique" />
              </div>
              <div className={capabilities.comments ? "grid grid-cols-3 gap-2" : "grid grid-cols-2 gap-2"}>
                <Input value={r.default ?? ""} onChange={(e) => update(r.key, { default: e.target.value })} aria-label="Default" placeholder="default" spellCheck={false} className="h-8 font-mono text-xs" />
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
