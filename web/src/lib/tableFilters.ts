import type { TableFilter } from "@/types";

export interface FilterOperator {
  value: string;
  label: string;
  /** Null checks have no value to type. */
  noValue?: boolean;
  placeholder?: string;
}

export const FILTER_OPERATORS: FilterOperator[] = [
  { value: "=", label: "equals" },
  { value: "!=", label: "does not equal" },
  { value: "like", label: "like", placeholder: "%text%" },
  { value: "not like", label: "not like", placeholder: "%text%" },
  { value: "ilike", label: "ilike", placeholder: "%text%" },
  { value: "not ilike", label: "not ilike", placeholder: "%text%" },
  { value: "<", label: "less than" },
  { value: "<=", label: "less than or equal" },
  { value: ">", label: "greater than" },
  { value: ">=", label: "greater than or equal" },
  { value: "in", label: "in list", placeholder: "1, 2, 3" },
  { value: "not in", label: "not in list", placeholder: "1, 2, 3" },
  { value: "is null", label: "is null", noValue: true },
  { value: "is not null", label: "is not null", noValue: true },
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
