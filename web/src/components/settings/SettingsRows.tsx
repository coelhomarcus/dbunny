import type { ReactNode } from "react";

/** One setting: what it is and what it does on the left, its control on the right. */
export function Row({ title, hint, children }: { title: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-border py-3.5 last:border-b-0">
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{title}</div>
        {hint && <div className="mt-0.5 text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return <h2 className="mb-1 text-[15px] font-semibold">{children}</h2>;
}
