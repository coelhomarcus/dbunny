import type { SessionSummary } from "../types/assistant";
import { fmt, t } from "../i18n/store.ts";

export interface DayGroup {
  label: string;
  entries: SessionSummary[];
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

function labelOf(date: Date, now: Date): string {
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days === 0) return t("assistant.history.today");
  if (days === 1) return t("assistant.history.yesterday");
  const sameYear = date.getFullYear() === now.getFullYear();
  return fmt().date(date, { dateStyle: undefined, month: "short", day: "numeric", year: sameYear ? undefined : "numeric" });
}

/** Past conversations grouped by the day they were last used, the newest day and conversation first; those with no date go last. */
export function groupByDay(entries: SessionSummary[], now: Date = new Date()): DayGroup[] {
  const dated: { entry: SessionSummary; date: Date }[] = [];
  const undated: SessionSummary[] = [];
  for (const entry of entries) {
    const date = entry.updatedAt ? new Date(entry.updatedAt) : null;
    if (date && !Number.isNaN(date.getTime())) dated.push({ entry, date });
    else undated.push(entry);
  }
  dated.sort((a, b) => b.date.getTime() - a.date.getTime());
  const groups: DayGroup[] = [];
  for (const { entry, date } of dated) {
    const label = labelOf(date, now);
    const last = groups[groups.length - 1];
    if (last && last.label === label) last.entries.push(entry);
    else groups.push({ label, entries: [entry] });
  }
  if (undated.length > 0) groups.push({ label: t("assistant.history.earlier"), entries: undated });
  return groups;
}
