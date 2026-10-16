import type { ComponentProps } from "react";
import { cn } from "../../lib/cn";

// Original tinted fills. The shadow matches the filled buttons, without a border
// or a light inset, which washed these small chips out.
const variants = {
  neutral: "bg-muted text-muted-foreground shadow-xs shadow-black/10",
  success: "bg-success/10 text-success-foreground shadow-xs shadow-success/25 dark:bg-success/16",
  warning: "bg-warning/10 text-warning-foreground shadow-xs shadow-warning/25 dark:bg-warning/16",
  destructive: "bg-destructive/10 text-destructive-foreground shadow-xs shadow-destructive/25 dark:bg-destructive/16",
  info: "bg-info/10 text-info-foreground shadow-xs shadow-info/25 dark:bg-info/16",
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
