export interface ShortcutEvent {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** Cmd+I on a Mac, Ctrl+I elsewhere: opens and closes the assistant. */
export function isAssistantShortcut(event: ShortcutEvent, mac: boolean): boolean {
  const modifier = mac ? event.metaKey : event.ctrlKey;
  return modifier && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "i";
}
