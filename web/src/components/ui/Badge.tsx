import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

// Tinted background + coloured text; a bit stronger in dark mode
const variants = {
  neutral: "bg-muted text-muted-foreground",
  success: "bg-success/10 text-success-foreground dark:bg-success/16",
  warning: "bg-warning/10 text-warning-foreground dark:bg-warning/16",
  destructive: "bg-destructive/10 text-destructive-foreground dark:bg-destructive/16",
  info: "bg-info/10 text-info-foreground dark:bg-info/16",
} as const;

export default function Badge({
  variant = "neutral",
  className,
  ...props
}: ComponentProps<"span"> & { variant?: keyof typeof variants }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 items-center gap-1 rounded-md px-1.5 text-xs font-medium whitespace-nowrap [&_svg]:size-3 [&_svg]:shrink-0",
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}
