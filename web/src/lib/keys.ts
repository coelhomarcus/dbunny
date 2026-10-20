const KEY = /Control|Ctrl|Shift|Alt|Option|Command|Cmd|Backspace|Delete|Return|Enter|Space|Esc|Tab|[⌘⇧⌥⌃↵⎋⌫⇥␣]|\S/gi;

/**
 * The keys of a shortcut written as text ("⇧⌘Z", "Ctrl+Shift+Z", "Ctrl Enter"), one for each: symbols
 * are keys of their own, words stay whole, and spaces and plus signs only separate (a plus last, or right
 * after a separator, is the key itself).
 */
export function splitShortcut(shortcut: string): string[] {
  const tokens = shortcut.match(KEY) ?? [];
  const keys: string[] = [];
  let afterSeparator = false;
  tokens.forEach((token, index) => {
    if (token === "+" && !afterSeparator && keys.length > 0 && index < tokens.length - 1) {
      afterSeparator = true;
      return;
    }
    keys.push(token);
    afterSeparator = false;
  });
  return keys;
}
