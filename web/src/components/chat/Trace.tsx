import { createContext, useContext, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "../../lib/cn";

export type TraceStatus = "running" | "error" | "pending";

const StatusContext = createContext<TraceStatus | undefined>(undefined);

/** The list of steps an agent took: tool calls, thinking, queries. Rows have one shape, so they read alike. */
export function Trace({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn("flex flex-col text-[13px]", className)}>{children}</div>;
}

/** One row: an icon, a title, and what is worth saying beside it. `status` says where the step stands. */
export function TraceLine({ status, className, children }: { status?: TraceStatus; className?: string; children: ReactNode }) {
  return (
    <StatusContext.Provider value={status}>
      <span
        data-status={status}
        className={cn(
          "flex min-h-7 w-full items-center gap-2 py-1 text-muted-foreground",
          status === "error" && "text-destructive-foreground",
          status === "pending" && "opacity-60",
          className,
        )}
      >
        {children}
      </span>
    </StatusContext.Provider>
  );
}

/** What the row is about, drawn before the title: decorative, since the title says it in words. */
export function TraceIcon({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <span aria-hidden="true" className={cn("flex size-4 shrink-0 items-center justify-center [&_svg]:size-3.5", className)}>
      {children}
    </span>
  );
}

/** One line of text, cut with an ellipsis when it is long; it shimmers while the step runs. */
export function TraceTitle({ className, children }: { className?: string; children: ReactNode }) {
  const status = useContext(StatusContext);
  return <span className={cn("min-w-0 truncate", status === "running" && "shimmer", className)}>{children}</span>;
}

/** What is said beside the title: a duration, a count. It is not cut. */
export function TraceDescription({ className, children }: { className?: string; children: ReactNode }) {
  return <span className={cn("shrink-0 text-xs tabular-nums text-muted-foreground/80", className)}>{children}</span>;
}

interface CollapsibleState {
  open: boolean;
  toggle: () => void;
}

const CollapsibleContext = createContext<CollapsibleState | null>(null);

/**
 * A row that opens. Uncontrolled with `defaultOpen`, or controlled with `open` and `onOpenChange`; to
 * open it while a step streams and still let the reader's click win, pass `open={choice ?? running}`.
 */
export function TraceCollapsible({
  open,
  defaultOpen = false,
  onOpenChange,
  className,
  children,
}: {
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  className?: string;
  children: ReactNode;
}) {
  const [inner, setInner] = useState(defaultOpen);
  const shown = open ?? inner;
  const toggle = () => {
    if (open === undefined) setInner(!shown);
    onOpenChange?.(!shown);
  };
  return (
    <CollapsibleContext.Provider value={{ open: shown, toggle }}>
      <div data-state={shown ? "open" : "closed"} className={className}>
        {children}
      </div>
    </CollapsibleContext.Provider>
  );
}

function useCollapsible(): CollapsibleState {
  const state = useContext(CollapsibleContext);
  if (!state) throw new Error("TraceCollapsibleTrigger and TraceCollapsibleContent go inside a TraceCollapsible");
  return state;
}

/** The button that opens the row; the chevron after its line turns when it is open. */
export function TraceCollapsibleTrigger({ className, children }: { className?: string; children: ReactNode }) {
  const { open, toggle } = useCollapsible();
  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={toggle}
      className={cn("group flex w-full cursor-pointer items-center rounded-md text-left hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring/50", className)}
    >
      {children}
      <ChevronRight aria-hidden="true" size={13} className={cn("ml-auto shrink-0 text-muted-foreground transition-transform", open && "rotate-90")} />
    </button>
  );
}

/** What the row shows when it is open. `keepMounted` keeps it, and what is open inside it, while closed. */
export function TraceCollapsibleContent({ keepMounted = false, className, children }: { keepMounted?: boolean; className?: string; children: ReactNode }) {
  const { open } = useCollapsible();
  if (!open && !keepMounted) return null;
  return (
    <div hidden={!open} className={cn("ml-2 border-l border-border pl-3.5 pt-0.5 pb-1.5 text-muted-foreground", className)}>
      {children}
    </div>
  );
}
