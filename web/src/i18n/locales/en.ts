/** The source of every message. A key is area.screen.role; words used everywhere live in `common`. */
export const en = {
  common: {
    back: "Back",
    cancel: "Cancel",
    close: "Close",
    copy: "Copy",
    delete: "Delete",
    retry: "Retry",
    save: "Save",
  },
  settings: {
    language: {
      label: "Language",
      hint: "The language of the app. System follows your computer.",
      system: "System",
    },
  },
} as const;
