import { useState, type FormEvent } from "react";
import type { ColumnInfo } from "@/types";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import Input from "../ui/Input";
import { selectClass } from "../ui/styles";
import { cn } from "../../lib/cn";
import { shortType } from "../../lib/columnMeta";
import { collectInsertValues, UNTOUCHED_INSERT_FIELD, type InsertField } from "../../lib/insertRowValues";

// <select> option values for "leave to the default"; must not collide with an enum label
const DEFAULT_OPTION = "\u0000default";

function InsertForm({
  schema,
  table,
  columns,
  initialValues,
  onInsert,
  onClose,
}: {
  schema: string;
  table: string;
  columns: ColumnInfo[];
  initialValues?: Record<string, string | null>;
  onInsert: (values: Record<string, string | null>) => Promise<void>;
  onClose: () => void;
}) {
  const [fields, setFields] = useState<Record<string, InsertField>>(() =>
    Object.fromEntries(
      Object.entries(initialValues ?? {}).map(([name, value]) => [
        name,
        { touched: true, isNull: value === null, text: value ?? "" },
      ]),
    ),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Generated columns are computed by the server and can't be given a value
  const ordered = columns.filter((c) => !c.isGenerated).sort((a, b) => a.ordinalPosition - b.ordinalPosition);

  function update(name: string, patch: Partial<InsertField>) {
    setFields((prev) => ({ ...prev, [name]: { ...(prev[name] ?? UNTOUCHED_INSERT_FIELD), ...patch } }));
  }

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onInsert(collectInsertValues(columns, fields));
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
          {initialValues ? "Clone row" : "Insert row"}{" "}
          <span className="font-normal text-muted-foreground">in {schema}.{table}</span>
        </>
      }
      description={
        initialValues
          ? "Filled in from the copied row. Keys with a default are left out so they get a new value."
          : "Fields you leave alone get the column's default (or NULL)."
      }
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
      <form onSubmit={submit} className="-mr-2 max-h-[55vh] space-y-2 overflow-y-auto pr-2">
        {ordered.map((column, index) => {
          const field = fields[column.name] ?? UNTOUCHED_INSERT_FIELD;
          const required = !column.isNullable && column.defaultValue === null;
          const choices = column.dataType === "boolean" ? ["true", "false"] : column.enumValues?.length ? column.enumValues : null;
          const placeholder = column.defaultValue !== null ? `default: ${column.defaultValue}` : column.isNullable ? "NULL" : "required";
          const clearNull = () => update(column.name, field.isNull ? { touched: false, isNull: false, text: "" } : { touched: true, isNull: true, text: "" });
          return (
            <div key={column.name} className="rounded-lg border border-border px-3 py-2.5">
              <label htmlFor={`insert-${column.name}`} className="mb-1.5 flex min-w-0 items-baseline gap-2">
                <span className="truncate font-mono text-[13px]">
                  {column.name}
                  {required && <span className="ml-0.5 text-destructive-foreground">*</span>}
                </span>
                <span className="truncate text-[11px] text-muted-foreground">
                  {column.isPrimaryKey && <span className="mr-1 text-warning-foreground">PK</span>}
                  {shortType(column.typeName)}
                </span>
              </label>
              <div className="flex items-center gap-2">
                {choices ? (
                  <select
                    id={`insert-${column.name}`}
                    disabled={field.isNull}
                    className={cn(selectClass, "min-w-0 flex-1", field.isNull && "text-muted-foreground")}
                    value={field.isNull ? "" : field.touched ? field.text : DEFAULT_OPTION}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (v === DEFAULT_OPTION) setFields((prev) => ({ ...prev, [column.name]: UNTOUCHED_INSERT_FIELD }));
                      else update(column.name, { touched: true, isNull: false, text: v });
                    }}
                  >
                    {field.isNull ? (
                      <option value="">NULL</option>
                    ) : (
                      <option value={DEFAULT_OPTION}>{column.defaultValue !== null ? `default: ${column.defaultValue}` : "(not set)"}</option>
                    )}
                    {choices.map((choice) => (
                      <option key={choice} value={choice}>
                        {choice}
                      </option>
                    ))}
                  </select>
                ) : (
                  <Input
                    id={`insert-${column.name}`}
                    value={field.isNull ? "" : field.text}
                    placeholder={field.isNull ? "NULL" : placeholder}
                    disabled={field.isNull}
                    autoFocus={index === 0}
                    spellCheck={false}
                    autoComplete="off"
                    onChange={(e) => update(column.name, { touched: true, isNull: false, text: e.target.value })}
                    className="min-w-0 flex-1 font-mono text-xs placeholder:italic"
                  />
                )}
                {column.isNullable && (
                  <button
                    type="button"
                    onClick={clearNull}
                    className={cn(
                      "h-8 shrink-0 rounded-lg px-2 text-[11px] font-medium cursor-pointer transition-colors",
                      field.isNull ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground hover:bg-accent hover:text-foreground",
                    )}
                  >
                    NULL
                  </button>
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
  /** Starting values, for cloning a row. */
  initialValues?: Record<string, string | null>;
  onInsert: (values: Record<string, string | null>) => Promise<void>;
  onClose: () => void;
}) {
  return open ? <InsertForm {...props} /> : null;
}
