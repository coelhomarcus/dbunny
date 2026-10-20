import type { Locale } from "./messages.ts";

type When = Date | number;
const MINUTE = 60_000;
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 60 * MINUTE],
  ["month", 30 * 24 * 60 * MINUTE],
  ["day", 24 * 60 * MINUTE],
  ["hour", 60 * MINUTE],
  ["minute", MINUTE],
];
const BYTE_UNITS = ["B", "kB", "MB", "GB", "TB"];

/** Numbers, dates and lists written the way a language writes them. */
export function makeFormatters(locale: Locale) {
  return {
    number(n: number, options?: Intl.NumberFormatOptions): string {
      return new Intl.NumberFormat(locale, options).format(n);
    },

    date(value: When, options?: Intl.DateTimeFormatOptions): string {
      return new Intl.DateTimeFormat(locale, { dateStyle: "medium", ...options }).format(value);
    },

    /** Hours and minutes. */
    time(value: When, options?: Intl.DateTimeFormatOptions): string {
      return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", ...options }).format(value);
    },

    dateTime(value: When, options?: Intl.DateTimeFormatOptions): string {
      return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short", ...options }).format(value);
    },

    /** "5 minutes ago", in the biggest unit that fits. */
    relative(value: When, now: number = Date.now()): string {
      const diff = Number(value) - now;
      const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
      for (const [unit, size] of UNITS) {
        if (Math.abs(diff) >= size) return format.format(Math.trunc(diff / size), unit);
      }
      return format.format(Math.trunc(diff / 1000), "second");
    },

    list(items: string[]): string {
      return new Intl.ListFormat(locale, { style: "long", type: "conjunction" }).format(items);
    },

    /** "1.4 MB": one decimal under 10, none above. */
    bytes(bytes: number): string {
      let value = bytes;
      let unit = 0;
      while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
        value /= 1024;
        unit++;
      }
      const digits = unit === 0 ? 0 : value < 10 ? 1 : 0;
      const text = new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
      return `${text} ${BYTE_UNITS[unit]}`;
    },
  };
}

export type Formatters = ReturnType<typeof makeFormatters>;
