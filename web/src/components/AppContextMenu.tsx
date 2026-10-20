import { useCallback, useEffect, useState } from "react";
import { ClipboardPaste, Copy, Redo2, Scissors, TextSelect, Undo2 } from "lucide-react";
import ContextMenu, { type ContextMenuItem } from "./ui/ContextMenu";
import { readClipboardText, writeClipboardText } from "../lib/clipboard";
import { isMac } from "../lib/platform";
import { t, useT } from "../i18n";

const MOD = isMac ? "⌘" : "Ctrl+";
const SHIFT = isMac ? "⇧" : "Shift+";

// Input types that hold text a person types (not checkboxes, ranges, colours...)
const TEXT_INPUTS = new Set(["text", "search", "url", "email", "tel", "password", "number", ""]);

type TextTarget = { element: HTMLElement; editable: boolean; secret: boolean; selected: string };

/** The text field (input, textarea or editable area such as the SQL editor) under the pointer. */
function textTargetOf(target: EventTarget | null): TextTarget | null {
  if (!(target instanceof Element)) return null;
  const field = target.closest<HTMLElement>("input, textarea, [contenteditable='true'], [contenteditable='']");
  if (!field) return null;
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    if (field instanceof HTMLInputElement && !TEXT_INPUTS.has(field.type)) return null;
    let selected = "";
    try {
      selected = field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0);
    } catch {
      // number inputs have no selection range
    }
    return {
      element: field,
      editable: !field.readOnly && !field.disabled,
      secret: field instanceof HTMLInputElement && field.type === "password",
      selected,
    };
  }
  return { element: field, editable: true, secret: false, selected: window.getSelection()?.toString() ?? "" };
}

/** Runs an editing command on the field, focused again first (the menu took the focus). */
function run(field: HTMLElement, command: "undo" | "redo" | "cut" | "copy" | "delete" | "selectAll") {
  field.focus();
  return document.execCommand(command);
}

/** Cut, copy and paste for a text field, the way the system menu offers them. */
function textItems({ element, editable, secret, selected }: TextTarget): ContextMenuItem[] {
  const copy = async () => {
    if (!run(element, "copy")) await writeClipboardText(selected);
  };
  const items: ContextMenuItem[] = [];
  if (editable) {
    items.push(
      { label: t("ui.contextMenu.undo"), icon: <Undo2 />, shortcut: `${MOD}Z`, onSelect: () => run(element, "undo") },
      { label: t("ui.contextMenu.redo"), icon: <Redo2 />, shortcut: `${SHIFT}${MOD}Z`, onSelect: () => run(element, "redo") },
      "separator",
      {
        label: t("ui.contextMenu.cut"),
        icon: <Scissors />,
        shortcut: `${MOD}X`,
        disabled: !selected || secret,
        onSelect: async () => {
          if (run(element, "cut")) return;
          await writeClipboardText(selected);
          run(element, "delete");
        },
      },
    );
  }
  items.push({ label: t("common.copy"), icon: <Copy />, shortcut: `${MOD}C`, disabled: !selected || secret, onSelect: copy });
  if (editable) {
    items.push({
      label: t("ui.contextMenu.paste"),
      icon: <ClipboardPaste />,
      shortcut: `${MOD}V`,
      onSelect: async () => {
        const text = await readClipboardText();
        element.focus();
        // As if typed: the field's own input handling and undo history see it
        if (text) document.execCommand("insertText", false, text);
      },
    });
  }
  items.push("separator", {
    label: t("ui.contextMenu.selectAll"),
    icon: <TextSelect />,
    shortcut: `${MOD}A`,
    onSelect: () => {
      if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
        element.focus();
        element.select();
      } else run(element, "selectAll");
    },
  });
  return items;
}

/**
 * The app's only right-click menu outside the ones pages open themselves: the browser's
 * menu (with Inspect) never shows. Text fields get cut, copy and paste; selected text
 * gets Copy; anywhere else a right-click does nothing.
 */
export default function AppContextMenu() {
  const t = useT();
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const close = useCallback(() => setMenu(null), []);

  useEffect(() => {
    // Listening on the window runs after the page's own handlers: a menu a page opened
    // has already called preventDefault, and is left alone
    function onContextMenu(e: MouseEvent) {
      if (e.defaultPrevented) return;
      // Development only: Shift+right-click keeps the browser's menu, to inspect the page
      if (import.meta.env.DEV && e.shiftKey) return;
      e.preventDefault();
      const field = textTargetOf(e.target);
      const selected = window.getSelection()?.toString() ?? "";
      const items = field
        ? textItems(field)
        : selected
          ? [{ label: t("common.copy"), icon: <Copy />, shortcut: `${MOD}C`, onSelect: () => writeClipboardText(selected) }]
          : [];
      setMenu(items.length ? { x: e.clientX, y: e.clientY, items } : null);
    }
    window.addEventListener("contextmenu", onContextMenu);
    return () => window.removeEventListener("contextmenu", onContextMenu);
  }, [t]);

  return <ContextMenu position={menu} items={menu?.items ?? []} onClose={close} />;
}
