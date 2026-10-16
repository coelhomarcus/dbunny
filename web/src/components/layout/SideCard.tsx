import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../../lib/cn";

/**
 * A card beside the page, opening and closing like the sidebar: width and the
 * gap animate over 300ms, and the content fades. Stays mounted through the close.
 */
export default function SideCard({
  open,
  widthClass,
  label,
  children,
}: {
  open: boolean;
  /** Tailwind width, also used for the inner pane so the text doesn't reflow while it slides. */
  widthClass: string;
  label: string;
  children: ReactNode;
}) {
  const [mounted, setMounted] = useState(false);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (open) setMounted(true);
    else setVisible(false);
  }, [open]);

  useEffect(() => {
    if (!open || !mounted) return;
    // One frame at width 0 so the transition has a start.
    const frame = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(frame);
  }, [open, mounted]);

  useEffect(() => {
    // Only after a close. While opening, `visible` stays false until the next frame,
    // and a background tab can delay that frame past this timeout.
    if (open || visible || !mounted) return;
    const timer = setTimeout(() => setMounted(false), 320);
    return () => clearTimeout(timer);
  }, [open, visible, mounted]);

  if (!mounted) return null;

  return (
    <aside
      inert={!visible}
      aria-label={label}
      className={cn(
        "relative shrink-0 overflow-hidden rounded-lg bg-sidebar transition-[width,margin] duration-300 ease-in-out",
        visible ? cn(widthClass, "border border-border") : "-ml-1.5 w-0",
      )}
    >
      <div
        className={cn(
          "absolute inset-y-0 right-0 flex min-h-0 flex-col transition-opacity duration-200",
          widthClass,
          visible ? "opacity-100 delay-75" : "opacity-0",
        )}
      >
        {children}
      </div>
    </aside>
  );
}
