import type { ConversationOptions } from "../types/assistant";
import type { Locale } from "../i18n/messages.ts";

/** What the person set for the assistant on one saved connection, kept in this computer's local storage. */
export interface ConnectionAssistantPrefs {
  /** The assistant sees column names and counts, never values. */
  schemaOnly: boolean;
  /** Facts about this database that the assistant should know. */
  instructions: string;
}

const STORAGE_KEY = "dbunny:assistant:connections";
const DEFAULTS: ConnectionAssistantPrefs = { schemaOnly: false, instructions: "" };
const MIN_ROWS = 10;
const MAX_ROWS = 1000;
const DEFAULT_ROWS = 100;

function readAll(): Record<string, unknown> {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return stored !== null && typeof stored === "object" && !Array.isArray(stored) ? stored : {};
  } catch {
    return {};
  }
}

/** The preferences of a saved connection; the defaults for one that wasn't saved. */
export function getConnectionPrefs(savedId: string | null): ConnectionAssistantPrefs {
  if (!savedId) return { ...DEFAULTS };
  const stored = readAll()[savedId];
  if (stored === null || typeof stored !== "object") return { ...DEFAULTS };
  const { schemaOnly, instructions } = stored as Record<string, unknown>;
  return {
    schemaOnly: typeof schemaOnly === "boolean" ? schemaOnly : DEFAULTS.schemaOnly,
    instructions: typeof instructions === "string" ? instructions : DEFAULTS.instructions,
  };
}

/** Keeps the preferences of a saved connection; a connection that wasn't saved keeps none. */
export function setConnectionPrefs(savedId: string | null, prefs: ConnectionAssistantPrefs) {
  if (!savedId) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...readAll(), [savedId]: prefs }));
  } catch {
    // not kept; they still apply to this session's conversations
  }
}

/** The row limit for what was typed in the field: a whole number from 10 to 1000, and the default for an empty or unreadable one. */
export function parseRowLimit(text: string): number {
  const typed = Number(text);
  if (text.trim() === "" || !Number.isFinite(typed)) return DEFAULT_ROWS;
  return Math.min(MAX_ROWS, Math.max(MIN_ROWS, Math.round(typed)));
}

/** What a new conversation is opened with. */
export function conversationOptions(
  settings: { assistantAskBeforeQueries: boolean; assistantRowLimit: number },
  prefs: ConnectionAssistantPrefs,
  language: Locale,
): ConversationOptions {
  const rows = Number.isFinite(settings.assistantRowLimit) ? Math.round(settings.assistantRowLimit) : DEFAULT_ROWS;
  return {
    askBeforeQueries: settings.assistantAskBeforeQueries,
    schemaOnly: prefs.schemaOnly,
    rowLimit: Math.min(MAX_ROWS, Math.max(MIN_ROWS, rows)),
    instructions: prefs.instructions,
    language,
  };
}
