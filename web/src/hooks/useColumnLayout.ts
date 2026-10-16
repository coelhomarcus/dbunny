import { useCallback, useMemo, useState } from "react";
import type { QueryColumn } from "@/types";

/** Which columns are hidden and the order they're shown in, remembered per table. */
interface ColumnLayout {
  hidden: string[];
  /** Column names in display order; columns not listed (new ones) go at the end. */
  order: string[];
}

const EMPTY: ColumnLayout = { hidden: [], order: [] };

function load(key: string | null): ColumnLayout {
  if (!key) return EMPTY;
  try {
    const parsed = JSON.parse(localStorage.getItem(key) ?? "null");
    return parsed && Array.isArray(parsed.hidden) && Array.isArray(parsed.order) ? parsed : EMPTY;
  } catch {
    return EMPTY;
  }
}

/** A column with its position in the row arrays returned by the server. */
export interface PlacedColumn {
  column: QueryColumn;
  index: number;
}

export function useColumnLayout(key: string | null, columns: QueryColumn[] | undefined) {
  const [layout, setLayout] = useState<ColumnLayout>(() => load(key));

  const save = useCallback(
    (next: ColumnLayout) => {
      setLayout(next);
      if (!key) return;
      try {
        if (next.hidden.length === 0 && next.order.length === 0) localStorage.removeItem(key);
        else localStorage.setItem(key, JSON.stringify(next));
      } catch {
        // kept for this view only
      }
    },
    [key],
  );

  /** Every column in display order, hidden ones included. */
  const ordered = useMemo<PlacedColumn[]>(() => {
    const placed = (columns ?? []).map((column, index) => ({ column, index }));
    const rank = (name: string) => {
      const i = layout.order.indexOf(name);
      return i === -1 ? Number.MAX_SAFE_INTEGER : i;
    };
    return placed.sort((a, b) => rank(a.column.name) - rank(b.column.name) || a.index - b.index);
  }, [columns, layout.order]);

  const visible = useMemo(
    () => ordered.filter((p) => !layout.hidden.includes(p.column.name)),
    [ordered, layout.hidden],
  );

  const toggle = useCallback(
    (name: string) => {
      const hidden = layout.hidden.includes(name)
        ? layout.hidden.filter((n) => n !== name)
        : [...layout.hidden, name];
      // Always keep one column on screen
      if (hidden.length >= (columns?.length ?? 0)) return;
      save({ ...layout, hidden });
    },
    [layout, columns, save],
  );

  /** Moves a column one place earlier (-1) or later (+1). */
  const move = useCallback(
    (name: string, step: -1 | 1) => {
      const names = ordered.map((p) => p.column.name);
      const from = names.indexOf(name);
      const to = from + step;
      if (from < 0 || to < 0 || to >= names.length) return;
      [names[from], names[to]] = [names[to], names[from]];
      save({ ...layout, order: names });
    },
    [ordered, layout, save],
  );

  const reset = useCallback(() => save(EMPTY), [save]);

  return {
    ordered,
    visible,
    hiddenCount: layout.hidden.filter((n) => columns?.some((c) => c.name === n)).length,
    isHidden: (name: string) => layout.hidden.includes(name),
    isCustom: layout.hidden.length > 0 || layout.order.length > 0,
    toggle,
    move,
    reset,
  };
}

export type ColumnLayoutApi = ReturnType<typeof useColumnLayout>;
