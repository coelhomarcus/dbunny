import { useState, type FormEvent } from "react";
import type { ColumnInfo } from "@/types";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import Input from "../ui/Input";
import { selectClass } from "../ui/styles";
import { cn } from "../../lib/cn";

type Field = {
  /** The user typed in it (even to clear it): an untouched field is left to the default. */
  touched: boolean;
  isNull: boolean;
  text: string;
};

const UNTOUCHED: Field = { touched: false, isNull: false, text: "" };

/** Values to insert: null is SQL NULL, and columns left alone are omitted. */
function collectValues(columns: ColumnInfo[], fields: Record<string, Field>) {
  const values: Record<string, string | null> = {};
  for (const column of columns) {
    const field = fields[column.name] ?? UNTOUCHED;
    if (field.isNull) values[column.name] = null;
    else if (field.touched) values[column.name] = field.text;
  }
  return values;
}

function InsertForm({
  schema,
  table,
  columns,
  onInsert,
  onClose,
}: {
  schema: string;
  table: string;
  columns: ColumnInfo[];
  onInsert: (values: Record<string, string | null>) => Promise<void>;
  onClose: () => void;
}) {
  const [fields, setFields] = useState<Record<string, Field>>({});
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ordered = [...columns].sort((a, b) => a.ordinalPosition - b.ordinalPosition);

  function update(name: string, patch: Partial<Field>) {
    setFields((prev) => ({ ...prev, [name]: { ...(prev[name] ?? UNTOUCHED), ...patch } }));
  }

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onInsert(collectValues(columns, fields));
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : err instanceof Error ? err.message : "Failed to insert the row");
      setSaving(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      className="max-w-xl"
      title={
        <>
          Insert row <span className="font-normal text-muted-foreground">in {schema}.{table}</span>
        </>
      }
      description="Fields you leave alone get the column's default (or NULL)."
      footer={
        <>
          {error && <p className="mr-auto min-w-0 flex-1 break-words text-xs text-destructive-foreground">{error}</p>}
          <Button variant="neutral" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={() => submit()} disabled={saving}>
            Insert
          </Button>
        </>
      }
    >
      <form onSubmit={submit} className="-mr-2 max-h-[55vh] space-y-2.5 overflow-y-auto pr-2">
        {ordered.map((column, index) => {
          const field = fields[column.name] ?? UNTOUCHED;
          const required = !column.isNullable && column.defaultValue === null;
          const isBoolean = column.dataType === "boolean";
          const placeholder = field.isNull
            ? "NULL"
            : column.defaultValue !== null
              ? `default: ${column.defaultValue}`
              : column.isNullable
                ? "NULL"
                : "required";
          return (
            <div key={column.name} className="grid grid-cols-[9.5rem_1fr] items-center gap-3">
              <label htmlFor={`insert-${column.name}`} className="min-w-0 select-none">
                <span className="block truncate font-mono text-[13px]">
                  {column.name}
                  {required && <span className="ml-0.5 text-destructive-foreground">*</span>}
                </span>
                <span className="block truncate text-[11px] text-muted-foreground">
                  {column.isPrimaryKey && <span className="mr-1 text-warning-foreground">PK</span>}
                  {column.dataType}
                </span>
              </label>

              <div className="relative">
                {isBoolean ? (
                  <select
                    id={`insert-${column.name}`}
                    className={cn(selectClass, "w-full")}
                    value={field.isNull ? "null" : field.touched ? field.text : ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (v === "") setFields((prev) => ({ ...prev, [column.name]: UNTOUCHED }));
                      else if (v === "null") update(column.name, { touched: true, isNull: true, text: "" });
                      else update(column.name, { touched: true, isNull: false, text: v });
                    }}
                  >
                    <option value="">{column.defaultValue !== null ? `default: ${column.defaultValue}` : "(not set)"}</option>
                    <option value="true">true</option>
                    <option value="false">false</option>
                    {column.isNullable && <option value="null">NULL</option>}
                  </select>
                ) : (
                  <>
                    <Input
                      id={`insert-${column.name}`}
                      value={field.text}
                      placeholder={placeholder}
                      autoFocus={index === 0}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(e) => update(column.name, { touched: true, isNull: false, text: e.target.value })}
                      className={cn("font-mono text-xs placeholder:italic", column.isNullable && "pr-14")}
                    />
                    {column.isNullable && (
                      <button
                        type="button"
                        tabIndex={-1}
                        title="Insert NULL"
                        onClick={() => update(column.name, { touched: true, isNull: true, text: "" })}
                        className={cn(
                          "absolute right-1.5 top-1/2 -translate-y-1/2 rounded px-1.5 py-0.5 text-[11px] font-medium cursor-pointer transition-colors",
                          field.isNull ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground hover:bg-accent hover:text-foreground",
                        )}
                      >
                        NULL
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>
          );
        })}
        {/* Lets Enter submit from any field */}
        <button type="submit" className="hidden" />
      </form>
    </Dialog>
  );
}

/** Form to insert a row. Mounted only while open, so it starts fresh every time. */
export default function InsertRowDialog({
  open,
  ...props
}: {
  open: boolean;
  schema: string;
  table: string;
  columns: ColumnInfo[];
  onInsert: (values: Record<string, string | null>) => Promise<void>;
  onClose: () => void;
}) {
  return open ? <InsertForm {...props} /> : null;
}
