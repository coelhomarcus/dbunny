import { cn } from "../../lib/cn";

export default function Switch({
  checked,
  onChange,
  id,
  className,
  "aria-label": ariaLabel,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  id?: string;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors cursor-pointer outline-none",
        "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
        checked ? "bg-primary" : "bg-foreground/15",
        className,
      )}
    >
      <span
        className={cn(
          "absolute left-0.5 size-4 rounded-full bg-white shadow-xs transition-transform",
          checked && "translate-x-4",
        )}
      />
    </button>
  );
}
