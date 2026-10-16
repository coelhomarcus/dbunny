/**
 * The system clipboard. In the app it goes through Tauri's clipboard plugin, which reads
 * without the web view's paste confirmation; in a plain browser (the dev preview) it falls
 * back to the browser's clipboard.
 */
export async function readClipboardText(): Promise<string> {
  try {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
    return await readText();
  } catch {
    return (await navigator.clipboard?.readText?.().catch(() => "")) ?? "";
  }
}

export async function writeClipboardText(text: string): Promise<void> {
  try {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(text);
  } catch {
    await navigator.clipboard?.writeText?.(text).catch(() => {});
  }
}
