/** How a date/time column is edited. */
export type DateMode = "date" | "timestamp" | "timestamptz" | "time";

export function dateModeOf(dataType: string | undefined): DateMode | null {
  switch (dataType) {
    case "date":
      return "date";
    case "timestamp without time zone":
      return "timestamp";
    case "timestamp with time zone":
      return "timestamptz";
    case "time without time zone":
      return "time";
    default:
      return null;
  }
}

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/**
 * The value as editable text: "2026-01-02", "2026-01-02 10:00:00", "10:00:00".
 * timestamptz is shown in UTC so the edit never depends on this computer's time zone.
 */
export function toDateText(value: unknown, mode: DateMode): string {
  if (value === null || value === undefined) return "";
  const text = String(value);
  if (mode === "timestamptz") {
    // "2026-01-02 10:00:00+0000" isn't ISO; WebKit only parses the ISO form reliably
    const iso = text.replace(" ", "T").replace(/([+-]\d\d)(\d\d)$/, "$1:$2");
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return text;
    const fraction = text.match(/\.(\d+)/)?.[1];
    return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())} ${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())}${fraction ? `.${fraction}` : ""}`;
  }
  if (mode === "timestamp") return text.replace("T", " ");
  return text;
}

/** Text to save: timestamptz gets "+00" since it was edited in UTC. */
export function fromDateText(text: string, mode: DateMode): string {
  const trimmed = text.trim();
  // Only dates written out get the offset; words like "infinity" or "now" stay as they are
  const isDate = /^\d{4}-\d{2}-\d{2}/.test(trimmed);
  return mode === "timestamptz" && isDate && !/[zZ]|[+-]\d\d(:?\d\d)?$/.test(trimmed) ? `${trimmed}+00` : trimmed;
}

/** "YYYY-MM-DD" part of the text, if it has one. */
export function datePart(text: string): { year: number; month: number; day: number } | null {
  const m = text.match(/^(\d{4})-(\d{2})-(\d{2})/);
  return m ? { year: +m[1], month: +m[2], day: +m[3] } : null;
}

/** Time part ("HH:MM:SS[.fff]") of a date/time text. */
export function timePart(text: string, mode: DateMode): string {
  if (mode === "time") return text;
  return text.match(/[ T](\d{2}:\d{2}(:\d{2}(\.\d+)?)?)/)?.[1] ?? "00:00:00";
}

export function withDate(text: string, mode: DateMode, year: number, month: number, day: number): string {
  const date = `${year}-${pad(month)}-${pad(day)}`;
  return mode === "date" ? date : `${date} ${timePart(text, mode)}`;
}

export function withTime(text: string, mode: DateMode, time: string): string {
  if (mode === "time") return time;
  const date = text.match(/^\d{4}-\d{2}-\d{2}/)?.[0];
  return date ? `${date} ${time}` : text;
}

/** The current moment as text for the mode (UTC for timestamptz, local otherwise). */
export function nowText(mode: DateMode): string {
  const d = new Date();
  const utc = mode === "timestamptz";
  const y = utc ? d.getUTCFullYear() : d.getFullYear();
  const mo = (utc ? d.getUTCMonth() : d.getMonth()) + 1;
  const da = utc ? d.getUTCDate() : d.getDate();
  const time = `${pad(utc ? d.getUTCHours() : d.getHours())}:${pad(utc ? d.getUTCMinutes() : d.getMinutes())}:${pad(utc ? d.getUTCSeconds() : d.getSeconds())}`;
  if (mode === "date") return `${y}-${pad(mo)}-${pad(da)}`;
  if (mode === "time") return time;
  return `${y}-${pad(mo)}-${pad(da)} ${time}`;
}
