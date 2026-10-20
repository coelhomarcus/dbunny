import type { ElementType, MouseEventHandler, ReactNode } from "react";
import { cn } from "../../lib/cn";

type MarkerVariant = "default" | "border" | "separator";

interface MarkerProps {
  /** `default`: a row of status or a note; `border`: with a line under it; `separator`: a label between two lines. */
  variant?: MarkerVariant;
  /** In progress or just changed: said aloud to those who can't see it. */
  live?: boolean;
  /** Makes the marker a link. */
  href?: string;
  /** Makes the marker a button. */
  onClick?: MouseEventHandler<HTMLElement>;
  /** Names a marker that has only an icon, or a separator. */
  label?: string;
  className?: string;
  children: ReactNode;
}

const base = "flex w-full items-center gap-2 text-xs leading-relaxed text-muted-foreground";

const variants: Record<MarkerVariant, string> = {
  default: "",
  border: "border-b border-border pb-2",
  // The two lines are decoration: pseudo-elements on each side of the label
  separator: "gap-3 before:h-px before:flex-1 before:bg-border after:h-px after:flex-1 after:bg-border",
};

/**
 * An inline row of the conversation that is not a message: a status, a note, a row with a line under it
 * or a label between two lines. Give it `href` or `onClick` and it is a link or a button.
 */
export function Marker({ variant = "default", live, href, onClick, label, className, children }: MarkerProps) {
  const Tag: ElementType = href ? "a" : onClick ? "button" : "div";
  const interactive = href || onClick;
  return (
    <Tag
      role={live ? "status" : undefined}
      aria-label={label}
      href={href}
      type={Tag === "button" ? "button" : undefined}
      target={href ? "_blank" : undefined}
      rel={href ? "noreferrer" : undefined}
      onClick={onClick}
      className={cn(base, variants[variant], interactive && "cursor-pointer rounded-md text-left hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring/50", className)}
    >
      {children}
    </Tag>
  );
}

/** The icon of a marker: decorative, so a marker with only an icon needs a `label`. */
export function MarkerIcon({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <span aria-hidden="true" className={cn("flex size-3.5 shrink-0 items-center justify-center [&_svg]:size-3.5", className)}>
      {children}
    </span>
  );
}

/** The text of a marker; with `shimmer` it brightens in a sweep, for what is still going on. */
export function MarkerContent({ shimmer, className, children }: { shimmer?: boolean; className?: string; children: ReactNode }) {
  return <span className={cn("min-w-0", shimmer && "shimmer", className)}>{children}</span>;
}
