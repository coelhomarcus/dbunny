import { useSyncExternalStore } from "react";
import type { Update } from "@tauri-apps/plugin-updater";
import { getSettings, subscribeSettings } from "./settings";

/**
 * App updates from the GitHub releases (the endpoint and signing key are in tauri.conf.json):
 * look for a newer version, download and install it, then restart into it. One state for the
 * whole app, read by the Settings page, the update notice and the status bar.
 */
export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "up-to-date"; checkedAt: number }
  | { status: "available"; version: string; notes: string | null; date: string | null }
  | { status: "downloading"; version: string; progress: number | null }
  | { status: "ready"; version: string }
  | { status: "error"; message: string; during: "check" | "install" };

/** How often the app looks again while it stays open. */
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
/** The first look waits a little, so it doesn't compete with the app starting up. */
const FIRST_CHECK_AFTER_MS = 5_000;

export const RELEASES_URL = "https://github.com/coelhomarcus/dbunny/releases";

let state: UpdateState = { status: "idle" };
// The version whose notice the person closed this session; it comes back for a newer one
let dismissed: string | null = null;
let pending: Update | null = null;
const listeners = new Set<() => void>();

function set(next: UpdateState) {
  state = next;
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Running inside the desktop app (not the browser preview), where the updater exists. */
const inApp = () => "__TAURI_INTERNALS__" in window;

const messageOf = (err: unknown) => (typeof err === "string" ? err : err instanceof Error ? err.message : "Something went wrong");

/**
 * Asks the release server for a newer version. A check the person started reports its
 * errors; an automatic one fails quietly (offline, say) and tries again later.
 */
export async function checkForUpdates({ manual = false } = {}) {
  if (state.status === "checking" || state.status === "downloading" || state.status === "ready") return;
  if (!inApp()) {
    if (manual) set({ status: "error", message: "Updates are only available in the desktop app", during: "check" });
    return;
  }
  const before = state;
  set({ status: "checking" });
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check({ timeout: 30_000 });
    pending = update;
    if (update) {
      set({ status: "available", version: update.version, notes: update.body ?? null, date: update.date ?? null });
    } else {
      set({ status: "up-to-date", checkedAt: Date.now() });
    }
  } catch (err) {
    if (manual) set({ status: "error", message: messageOf(err), during: "check" });
    else set(before);
  }
}

/**
 * Downloads and installs the update found by the last check. On Windows the installer closes
 * the app and opens the new version; elsewhere the app keeps running until it's restarted.
 */
export async function installUpdate() {
  if (!pending || state.status !== "available") return;
  const version = pending.version;
  let total = 0;
  let received = 0;
  dismissed = null;
  set({ status: "downloading", version, progress: null });
  try {
    await pending.downloadAndInstall((event) => {
      if (event.event === "Started") {
        total = event.data.contentLength ?? 0;
      } else if (event.event === "Progress") {
        received += event.data.chunkLength;
        set({ status: "downloading", version, progress: total ? Math.min(1, received / total) : null });
      }
    });
    set({ status: "ready", version });
  } catch (err) {
    set({ status: "error", message: messageOf(err), during: "install" });
  }
}

/** Opens the newly installed version. */
export async function restartApp() {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

/** Hides the update notice until the app opens again (a failed install is cleared). */
export function dismissUpdate() {
  if (state.status === "available" || state.status === "ready") dismissed = state.version;
  else if (state.status === "error") {
    set({ status: "idle" });
    return;
  }
  listeners.forEach((l) => l());
}

/** Shows the update notice again (from the status bar). */
export function showUpdate() {
  dismissed = null;
  listeners.forEach((l) => l());
}

export function useUpdate() {
  const current = useSyncExternalStore(subscribe, () => state);
  const isDismissed = useSyncExternalStore(
    subscribe,
    () => (state.status === "available" || state.status === "ready") && dismissed === state.version,
  );
  return { state: current, dismissed: isDismissed };
}

let started = false;

/**
 * Looks for updates when the app opens and every few hours, while "Check for updates
 * automatically" is on. Development builds don't look on their own.
 */
export function startUpdateChecks() {
  if (started || import.meta.env.DEV || !inApp()) return;
  started = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const schedule = (delay: number) => {
    clearTimeout(timer);
    if (!getSettings().checkForUpdates) return;
    timer = setTimeout(async () => {
      await checkForUpdates();
      schedule(CHECK_EVERY_MS);
    }, delay);
  };
  schedule(FIRST_CHECK_AFTER_MS);
  // Turning it back on looks right away; turning it off stops the timer
  let enabled = getSettings().checkForUpdates;
  subscribeSettings(() => {
    const now = getSettings().checkForUpdates;
    if (now !== enabled) schedule(now ? 0 : CHECK_EVERY_MS);
    enabled = now;
  });
}
