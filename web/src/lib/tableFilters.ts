import type { TableFilter } from "@/types";
import type { StaticKey } from "../i18n/store.ts";

export interface FilterOperator {
  value: string;
  labelKey: StaticKey;
  /** Null checks have no value to type. */
  noValue?: boolean;
  placeholder?: string;
  /** A placeholder that is words, not a literal example. */
  placeholderKey?: StaticKey;
}

export const FILTER_OPERATORS: FilterOperator[] = [
  { value: "=", labelKey: "table.operators.equals" },
  { value: "!=", labelKey: "table.operators.notEquals" },
  { value: "like", labelKey: "table.operators.like", placeholderKey: "table.operators.likePlaceholder" },
  { value: "not like", labelKey: "table.operators.notLike", placeholderKey: "table.operators.likePlaceholder" },
  { value: "ilike", labelKey: "table.operators.ilike", placeholderKey: "table.operators.likePlaceholder" },
  { value: "not ilike", labelKey: "table.operators.notIlike", placeholderKey: "table.operators.likePlaceholder" },
  { value: "<", labelKey: "table.operators.lessThan" },
  { value: "<=", labelKey: "table.operators.lessOrEqual" },
  { value: ">", labelKey: "table.operators.greaterThan" },
  { value: ">=", labelKey: "table.operators.greaterOrEqual" },
  { value: "in", labelKey: "table.operators.inList", placeholder: "1, 2, 3" },
  { value: "not in", labelKey: "table.operators.notInList", placeholder: "1, 2, 3" },
  { value: "is null", labelKey: "table.operators.isNull", noValue: true },
  { value: "is not null", labelKey: "table.operators.isNotNull", noValue: true },
];

export function operatorFor(value: string): FilterOperator {
  return FILTER_OPERATORS.find((op) => op.value === value) ?? FILTER_OPERATORS[0];
}

/** A filter is only sent once it has a column and, unless it's a null check, a value. */
export function isActiveFilter(filter: TableFilter): boolean {
  if (!filter.column) return false;
  return operatorFor(filter.operator).noValue || filter.value.trim() !== "";
}

export function emptyFilter(column = ""): TableFilter {
  return { column, operator: "=", value: "" };
}
