import { Bot, Terminal } from "lucide-react";
import type { AgentKind } from "../../lib/assistantAgents";
import { cn } from "../../lib/cn";
import ClaudeCodeIcon from "./ClaudeCodeIcon";

/** The mark of an agent: Claude Code's own, and a generic one for the others (no brand mark is borrowed for them). */
export default function AgentIcon({ kind, size = 15, className }: { kind: AgentKind; size?: number; className?: string }) {
  if (kind === "claude") return <ClaudeCodeIcon size={size} className="shrink-0 text-[#d97757]" />;
  const Icon = kind === "codex" ? Bot : Terminal;
  return <Icon size={size} strokeWidth={size > 24 ? 1.5 : 2} className={cn("shrink-0 text-muted-foreground", className)} />;
}
