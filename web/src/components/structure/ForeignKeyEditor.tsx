import { ArrowRight } from "lucide-react";
import type { CompletionRelation } from "@/types";
import { guessReference, REFERENTIAL_ACTIONS, type ForeignKeyDraft } from "../../lib/foreignKeyForm";
import { cn } from "../../lib/cn";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";
import { useT } from "../../i18n";

type Props = {
  relations: CompletionRelation[] | null;
  value: ForeignKeyDraft;
  onChange: (patch: Partial<ForeignKeyDraft>) => void;
  /** Columns on this table that participate in the key. */
  localColumnNames: string[];
  /** Create-table mode: pick local columns with checkboxes. Structure mode: list comes from ColumnPicker above. */
  selectableLocalColumns?: boolean;
  error?: string | null;
  matchingColumnsLabel?: string;
};

/** References table, column mapping, and ON DELETE / ON UPDATE for a new foreign key. */
export default function ForeignKeyEditor({
  relations,
  value,
  onChange,
  localColumnNames,
  selectableLocalColumns = false,
  error,
  matchingColumnsLabel,
}: Props) {
  const t = useT();
  const target = relations?.find((r) => `${r.schema}.${r.name}` === value.refTable);
  const keyColumns = selectableLocalColumns ? value.columns.filter((c) => localColumnNames.includes(c)) : localColumnNames;

  return (
    <div className="flex flex-col gap-2">
      {selectableLocalColumns && (
        <div className="flex flex-col gap-1">
          <span className="text-[13px] font-medium">{t("structure.foreignKeyEditor.columns")}</span>
          {localColumnNames.length === 0 ? (
            <p className="text-xs text-muted-foreground">{t("structure.foreignKeyEditor.nameAColumnFirst")}</p>
          ) : (
            <div className="flex flex-col gap-1">
              {localColumnNames.map((name) => {
                const on = value.columns.includes(name);
                const toggle = () =>
                  onChange({ columns: on ? value.columns.filter((c) => c !== name) : [...value.columns, name] });
                return (
                  <div key={name} className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-accent" onClick={toggle}>
                    <Checkbox checked={on} onChange={toggle} />
                    <span className="font-mono text-xs">{name}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
      <label className="flex flex-col gap-1.5">
        <span className="text-[13px] font-medium">{t("structure.foreignKeyEditor.references")}</span>
        <select
          value={value.refTable}
          onChange={(e) => onChange({ refTable: e.target.value, refColumns: {} })}
          className={cn(selectClass, "w-full")}
        >
          <option value="">{relations === null ? t("common.loading") : t("structure.foreignKeyEditor.chooseATable")}</option>
          {(relations ?? []).map((r) => (
            <option key={`${r.schema}.${r.name}`} value={`${r.schema}.${r.name}`}>
              {r.schema}.{r.name}
            </option>
          ))}
        </select>
      </label>
      {target && keyColumns.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium">{matchingColumnsLabel ?? t("structure.foreignKeyEditor.matchingColumns")}</span>
          {keyColumns.map((c) => (
            <div key={c} className="grid grid-cols-[1fr_auto_1fr] items-center gap-2">
              <span className="truncate font-mono text-xs">{c}</span>
              <ArrowRight size={13} className="text-muted-foreground" />
              <select
                value={value.refColumns[c] ?? guessReference(c, target)}
                onChange={(e) => onChange({ refColumns: { ...value.refColumns, [c]: e.target.value } })}
                className={cn(selectClass, "w-full font-mono text-xs")}
                aria-label={t("structure.foreignKeyEditor.referencedColumn", { column: c })}
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
      <div className={selectableLocalColumns ? "grid grid-cols-2 gap-2" : "grid grid-cols-2 gap-3"}>
        <label className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium">{t("structure.foreignKeyEditor.onDelete")}</span>
          <select value={value.onDelete} onChange={(e) => onChange({ onDelete: e.target.value })} className={cn(selectClass, "w-full")}>
            {REFERENTIAL_ACTIONS.map((a) => (
              <option key={a} value={a}>
                {a.toLowerCase()}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium">{t("structure.foreignKeyEditor.onUpdate")}</span>
          <select value={value.onUpdate} onChange={(e) => onChange({ onUpdate: e.target.value })} className={cn(selectClass, "w-full")}>
            {REFERENTIAL_ACTIONS.map((a) => (
              <option key={a} value={a}>
                {a.toLowerCase()}
              </option>
            ))}
          </select>
        </label>
      </div>
      {error && <p className="select-text text-xs text-destructive-foreground">{error}</p>}
    </div>
  );
}
