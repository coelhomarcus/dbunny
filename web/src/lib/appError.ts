import { has, tr } from "../i18n/store.ts";
import type { Params } from "../i18n/messages.ts";

/** An error from the backend: its english text, and, when DBunny made it, a code and the parameters of its message. */
export class AppError extends Error {
  code: string | null;
  params: Params;

  constructor(message: string, code: string | null = null, params: Params = {}) {
    super(message);
    this.name = "AppError";
    this.code = code;
    this.params = params;
  }
}

/** What a rejected command carries: the object from Rust, a plain text from older code, an Error. */
export function toAppError(raw: unknown): AppError {
  if (raw instanceof AppError) return raw;
  if (raw instanceof Error) return new AppError(raw.message);
  if (typeof raw === "string") return new AppError(raw);
  if (raw !== null && typeof raw === "object" && typeof (raw as { message?: unknown }).message === "string") {
    const { message, code, params } = raw as { message: string; code?: unknown; params?: unknown };
    return new AppError(message, typeof code === "string" ? code : null, params !== null && typeof params === "object" ? (params as Params) : {});
  }
  return new AppError(String(raw));
}

/** What to show for an error: a code DBunny knows, in the language of the app; else the text it came with; else the fallback. */
export function errorText(error: unknown, fallback = ""): string {
  if (error === undefined || error === null) return fallback;
  if (typeof error === "object" && !(error instanceof Error) && typeof (error as { message?: unknown }).message !== "string") return fallback;
  const known = toAppError(error);
  if (known.code && has(`errors.${known.code}`)) return tr(`errors.${known.code}`, known.params);
  return known.message || fallback;
}
