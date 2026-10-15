import { useSyncExternalStore } from "react";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

// Also read by the inline script in index.html, keep the key in sync
const STORAGE_KEY = "dbunny-theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");
const listeners = new Set<() => void>();

function readPreference(): ThemePreference {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    // storage unavailable: fall back to the system theme
  }
  return "system";
}

let preference = readPreference();

function resolve(pref: ThemePreference): ResolvedTheme {
  return pref === "system" ? (media.matches ? "dark" : "light") : pref;
}

function apply() {
  document.documentElement.classList.toggle("dark", resolve(preference) === "dark");
  listeners.forEach((l) => l());
}

media.addEventListener("change", () => {
  if (preference === "system") apply();
});

export function setThemePreference(next: ThemePreference) {
  preference = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // not persisted, still applied for this session
  }
  apply();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useTheme() {
  const pref = useSyncExternalStore(subscribe, () => preference);
  const resolved = useSyncExternalStore(subscribe, () => resolve(preference));
  return { preference: pref, resolved, setPreference: setThemePreference };
}

apply();
