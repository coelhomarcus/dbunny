import type { ColumnInfo } from "@/types";
import { t } from "../i18n/store.ts";

const SHORT_TYPES: [RegExp, string][] = [
  [/^character varying/, "varchar"],
  [/^character\b/, "char"],
  [/^timestamp(\(\d+\))? with time zone/, "timestamptz$1"],
  [/^timestamp(\(\d+\))? without time zone/, "timestamp$1"],
  [/^time(\(\d+\))? with time zone/, "timetz$1"],
  [/^time(\(\d+\))? without time zone/, "time$1"],
  [/^double precision/, "float8"],
  [/^bit varying/, "varbit"],
];

/** Compact type for the column header: "varchar(80)", "timestamptz", "mood". */
export function shortType(typeName: string): string {
  // Enums and other types created in a temp schema come back schema-qualified
  let type = typeName.replace(/^pg_temp_\d+\./, "");
  for (const [pattern, short] of SHORT_TYPES) type = type.replace(pattern, short);
  return type;
}

/** "public.users(id)" or, for a composite key, "public.regions(country, code)". */
export function foreignKeyTarget(column: ColumnInfo): string | null {
  const fk = column.foreignKey;
  if (!fk) return null;
  return `${fk.schema}.${fk.table}(${fk.refColumns.join(", ")})`;
}

/** What the header hint says about a column, one fact per line. */
export function columnFacts(column: ColumnInfo): string[] {
  const facts: string[] = [];
  if (column.isPrimaryKey) facts.push(t("ui.columnMeta.primaryKey"));
  const target = foreignKeyTarget(column);
  if (target) facts.push(t("ui.columnMeta.references", { target }));
  if (column.isGenerated) facts.push(t("ui.columnMeta.generated"));
  if (column.comment) facts.push(column.comment);
  return facts;
}

export function isJsonType(column: ColumnInfo | undefined) {
  return column?.dataType === "json" || column?.dataType === "jsonb";
}
