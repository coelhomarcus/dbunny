import { useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "../../lib/cn";
import { activatePseudo, setLanguage, useLocale, useT } from "../../i18n";
import { useSettings } from "../../lib/settings";
import type { Language } from "../../i18n/messages.ts";
import Popover from "../ui/Popover";

const LANGUAGES: Language[] = ["system", "en", "pt-BR"];

/** The language of the app: a button with the current one that lists the others, each in its own language. */
export default function LanguagePicker() {
  const t = useT();
  useLocale();
  const [{ language }] = useSettings();
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  // Names of languages are never translated; only "System" is
  const label = (value: Language) => (value === "system" ? t("settings.language.system") : value === "en" ? "English" : "Português (Brasil)");

  return (
    <div ref={anchor}>
      <button
        type="button"
        aria-label={t("settings.language.label")}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-9 w-full max-w-xs cursor-pointer items-center gap-2.5 rounded-lg border border-input bg-card px-3 text-left text-[13px] font-medium outline-none hover:bg-accent focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 dark:bg-muted"
      >
        <span className="min-w-0 flex-1 truncate">{label(language)}</span>
        <ChevronDown size={14} className="shrink-0 text-muted-foreground" />
      </button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchor} className="w-56 p-1">
        {LANGUAGES.map((value) => (
          <button
            key={value}
            type="button"
            role="menuitemradio"
            aria-checked={value === language}
            onClick={() => {
              setOpen(false);
              if (value !== language) void setLanguage(value);
            }}
            className={cn("flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-left text-[13px] font-medium hover:bg-accent", value === language && "bg-accent/60")}
          >
            <span className="min-w-0 flex-1 truncate">{label(value)}</span>
            {value === language && <Check size={13} className="shrink-0" />}
          </button>
        ))}
        {/* Development only: English made to look foreign and longer */}
        {import.meta.env.DEV && (
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              activatePseudo();
            }}
            className="mt-1 flex w-full cursor-pointer items-center gap-2.5 rounded-md border-t border-border px-2 py-2 text-left text-[13px] font-medium text-muted-foreground hover:bg-accent"
          >
            {"Pseudo (dev)" /* i18n-ignore: development only */}
          </button>
        )}
      </Popover>
    </div>
  );
}
