import type { CSSProperties } from "react";
import { Database, FileBox, Server, type LucideIcon } from "lucide-react";
import type { EngineCardId } from "../../lib/engines";
import { cn } from "../../lib/cn";

// Each database's mark, tinted with its usual colour
const MARKS: Record<EngineCardId, { Icon: LucideIcon; color: string }> = {
  postgres: { Icon: Database, color: "#4169e1" },
  sqlite: { Icon: FileBox, color: "#0f80cc" },
  mysql: { Icon: Server, color: "#00758f" },
};

/** A database's mark in a tile tinted with its colour. */
export default function EngineLogo({ engine, size = "md", className }: { engine: EngineCardId; size?: "sm" | "md"; className?: string }) {
  const { Icon, color } = MARKS[engine];
  const style: CSSProperties = {
    color,
    backgroundColor: `color-mix(in srgb, ${color} 14%, transparent)`,
    boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 24%, transparent)`,
  };
  return (
    <span
      className={cn(
        "flex shrink-0 items-center justify-center rounded-lg",
        size === "md" ? "size-10 [&_svg]:size-5" : "size-7 [&_svg]:size-3.5",
        className,
      )}
      style={style}
    >
      <Icon />
    </span>
  );
}
