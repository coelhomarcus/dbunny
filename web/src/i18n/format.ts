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

const BUILT = new Map<string, unknown>();

/**
 * How each kind of formatter is made. The options come with each call, so these cannot be constants;
 * `built` keeps what they make, which is why the rule about rebuilding does not apply to them.
 */
const MAKE = {
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  number: (locale: Locale, options?: Intl.NumberFormatOptions) => new Intl.NumberFormat(locale, options),
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  date: (locale: Locale, options?: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale, { dateStyle: "medium", ...options }),
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  time: (locale: Locale, options?: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", ...options }),
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  dateTime: (locale: Locale, options?: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short", ...options }),
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  relative: (locale: Locale) => new Intl.RelativeTimeFormat(locale, { numeric: "auto" }),
  // react-doctor-disable-next-line react-doctor/js-hoist-intl
  list: (locale: Locale) => new Intl.ListFormat(locale, { style: "long", type: "conjunction" }),
};

/** An `Intl` formatter is costly to build, so one is kept for each kind, language and set of options. */
function built<T>(kind: string, locale: Locale, options: object | undefined, make: () => T): T {
  const key = `${kind}|${locale}|${JSON.stringify(options ?? {})}`;
  let found = BUILT.get(key) as T | undefined;
  if (found === undefined) {
    found = make();
    BUILT.set(key, found);
  }
  return found;
}

/** Numbers, dates and lists written the way a language writes them. */
export function makeFormatters(locale: Locale) {
  return {
    number(n: number, options?: Intl.NumberFormatOptions): string {
      return built("number", locale, options, () => MAKE.number(locale, options)).format(n);
    },

    date(value: When, options?: Intl.DateTimeFormatOptions): string {
      return built("date", locale, options, () => MAKE.date(locale, options)).format(value);
    },

    /** Hours and minutes. */
    time(value: When, options?: Intl.DateTimeFormatOptions): string {
      return built("time", locale, options, () => MAKE.time(locale, options)).format(value);
    },

    dateTime(value: When, options?: Intl.DateTimeFormatOptions): string {
      return built("dateTime", locale, options, () => MAKE.dateTime(locale, options)).format(value);
    },

    /** "5 minutes ago", in the biggest unit that fits. */
    relative(value: When, now: number = Date.now()): string {
      const diff = Number(value) - now;
      const format = built("relative", locale, undefined, () => MAKE.relative(locale));
      for (const [unit, size] of UNITS) {
        if (Math.abs(diff) >= size) return format.format(Math.trunc(diff / size), unit);
      }
      return format.format(Math.trunc(diff / 1000), "second");
    },

    list(items: string[]): string {
      return built("list", locale, undefined, () => MAKE.list(locale)).format(items);
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
      const text = built("number", locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }, () => MAKE.number(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits })).format(value);
      return `${text} ${BYTE_UNITS[unit]}`;
    },
  };
}

export type Formatters = ReturnType<typeof makeFormatters>;
