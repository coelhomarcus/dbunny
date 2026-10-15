import type { ReactNode } from "react";
import { cn } from "../../lib/cn";

/** 48px header bar at the top of a page (same height as the sidebar header). */
export function PageHeader({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <header className={cn("flex h-12 shrink-0 items-center gap-3 border-b border-border px-4 select-none", className)}>
      {children}
    </header>
  );
}

/** Two-line title: a small muted label above the value ("Table / users"). */
export function HeaderTitle({
  label,
  icon,
  children,
  className,
}: {
  label: ReactNode;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex min-w-0 items-center gap-2.5", className)}>
      {icon && <span className="shrink-0 text-muted-foreground [&_svg]:size-4">{icon}</span>}
      <div className="min-w-0">
        <div className="truncate text-[11px] leading-tight text-muted-foreground">{label}</div>
        <div className="truncate text-sm font-medium leading-tight">{children}</div>
      </div>
    </div>
  );
}
