import { useSyncExternalStore } from "react";

/** App preferences, kept in this computer's local storage. The theme has its own store (lib/theme). */
export interface Settings {
  /** Look for a new version when the app opens and every few hours. */
  checkForUpdates: boolean;
  /** New query tabs start with safe mode on (it asks before destructive statements). */
  safeModeByDefault: boolean;
}

const DEFAULTS: Settings = {
  checkForUpdates: true,
  safeModeByDefault: true,
};

const STORAGE_KEY = "dbunny:settings";
const listeners = new Set<() => void>();

function read(): Settings {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    // Settings added later fall back to their defaults
    return { ...DEFAULTS, ...(stored && typeof stored === "object" ? stored : {}) };
  } catch {
    return { ...DEFAULTS };
  }
}

let current = read();

export function getSettings(): Settings {
  return current;
}

export function updateSettings(changes: Partial<Settings>) {
  current = { ...current, ...changes };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(current));
  } catch {
    // not persisted, still applied for this session
  }
  listeners.forEach((l) => l());
}

export function subscribeSettings(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useSettings(): [Settings, (changes: Partial<Settings>) => void] {
  return [useSyncExternalStore(subscribeSettings, getSettings), updateSettings];
}
