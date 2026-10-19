import { createContext, useContext } from "react";

export type SettingsSection = "general" | "editor" | "assistant" | "updates" | "about";

export interface SettingsDialogValue {
  /** Opens Settings, on a section (the last one seen when omitted). */
  openSettings: (section?: SettingsSection) => void;
}

export const SettingsDialogContext = createContext<SettingsDialogValue | null>(null);

export function useSettingsDialog(): SettingsDialogValue {
  const value = useContext(SettingsDialogContext);
  if (!value) throw new Error("useSettingsDialog must be used inside SettingsProvider");
  return value;
}
