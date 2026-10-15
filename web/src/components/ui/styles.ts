// Class helpers for the UI components. Kept out of the component files so
// Vite fast refresh keeps working (component files must only export components).
import { cn } from "../../lib/cn";

const buttonBase =
  "relative inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border " +
  "font-medium outline-none select-none cursor-pointer transition-[background-color,box-shadow,color] duration-150 " +
  "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-1 focus-visible:ring-offset-background " +
  "disabled:pointer-events-none disabled:opacity-55 " +
  "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

// Filled buttons get a 1px light line on top and a shadow tinted with their own
// colour; pressing swaps the line for a dark inset so the button looks pushed in.
const variants = {
  primary:
    "border-primary bg-primary text-primary-foreground shadow-xs shadow-primary/25 " +
    "inset-shadow-[0_1px_rgb(255_255_255/0.2)] hover:bg-primary/90 " +
    "active:inset-shadow-[0_1px_rgb(0_0_0/0.12)] active:shadow-none",
  destructive:
    "border-destructive bg-destructive text-white shadow-xs shadow-destructive/25 " +
    "inset-shadow-[0_1px_rgb(255_255_255/0.18)] hover:bg-destructive/90 " +
    "active:inset-shadow-[0_1px_rgb(0_0_0/0.12)] active:shadow-none",
  outline:
    "border-input bg-card text-foreground shadow-xs/5 hover:bg-card-hover " +
    "dark:inset-shadow-[0_1px_rgb(255_255_255/0.04)] active:shadow-none",
  secondary: "border-transparent bg-secondary text-secondary-foreground hover:bg-accent",
  ghost: "border-transparent text-foreground hover:bg-accent data-[active=true]:bg-accent",
  "ghost-muted":
    "border-transparent text-muted-foreground hover:bg-accent hover:text-foreground " +
    "data-[active=true]:bg-accent data-[active=true]:text-foreground",
  "destructive-ghost": "border-transparent text-destructive-foreground hover:bg-destructive/10",
} as const;

const sizes = {
  xs: "h-6 gap-1 rounded-md px-2 text-xs [&_svg:not([class*='size-'])]:size-3.5",
  sm: "h-7 px-2.5 text-[13px] [&_svg:not([class*='size-'])]:size-3.5",
  md: "h-8 px-3 text-sm",
  lg: "h-9 px-3.5 text-sm",
  "icon-xs": "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3.5",
  "icon-sm": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
  icon: "size-8",
} as const;

export type ButtonVariant = keyof typeof variants;
export type ButtonSize = keyof typeof sizes;

export function buttonClass(variant: ButtonVariant = "outline", size: ButtonSize = "md", className?: string) {
  return cn(buttonBase, variants[variant], sizes[size], className);
}

export const inputClass =
  "h-8 w-full min-w-0 rounded-lg border border-input bg-card px-2.5 text-sm text-foreground shadow-xs/5 " +
  "outline-none transition-[border-color,box-shadow] placeholder:text-muted-foreground/70 " +
  "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 " +
  "disabled:opacity-55 dark:bg-muted";

export const segmentedListClass = "flex w-fit items-center gap-0.5 rounded-lg bg-muted p-0.5";

export function segmentedItemClass(active: boolean) {
  return cn(
    "flex h-7 items-center justify-center gap-1.5 rounded-md px-3 text-[13px] font-medium cursor-pointer select-none outline-none",
    "focus-visible:ring-2 focus-visible:ring-ring/50 [&_svg]:size-3.5",
    active
      ? "bg-card text-foreground shadow-xs/10 dark:bg-accent dark:inset-shadow-[0_1px_rgb(255_255_255/0.04)]"
      : "text-muted-foreground hover:text-foreground hover:transition-colors",
  );
}
