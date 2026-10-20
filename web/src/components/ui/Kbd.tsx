import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { splitShortcut } from "../../lib/keys";

type KbdTone = "muted" | "inverse";

const tones: Record<KbdTone, string> = {
  // On the app's surfaces
  muted: "border-border/70 bg-muted text-muted-foreground",
  // Inside a filled button, in the color of its text
  inverse: "border-primary-foreground/20 bg-primary-foreground/15 text-primary-foreground",
};

/** One key, drawn as a small keycap: a soft fill and a line a little thicker under it. */
export function Kbd({ tone = "muted", className, children }: { tone?: KbdTone; className?: string; children: ReactNode }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] select-none items-center justify-center rounded-[5px] border border-b-2 px-1 font-sans text-[10.5px] font-medium leading-none",
        tones[tone],
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/** Keys that are pressed together, side by side. */
export function KbdGroup({ className, children }: { className?: string; children: ReactNode }) {
  return <span className={cn("inline-flex items-center gap-0.5", className)}>{children}</span>;
}

/** A shortcut written as text ("⌘↵", "Ctrl+Shift+F"), as a key for each of its parts. */
export function Keys({ keys, tone, className }: { keys: string; tone?: KbdTone; className?: string }) {
  return (
    <KbdGroup className={className}>
      {splitShortcut(keys).map((key, index) => (
        <Kbd key={index} tone={tone}>
          {key}
        </Kbd>
      ))}
    </KbdGroup>
  );
}
