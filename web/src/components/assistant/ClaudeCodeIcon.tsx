import type { LucideProps } from "lucide-react";

/**
 * The Claude Code mark, to say that the assistant runs Claude Code. Its shape is from the open
 * icon set of LobeHub (MIT); the mark itself belongs to Anthropic. Drawn in the current text color.
 */
export default function ClaudeCodeIcon({ size = 16, className }: Pick<LucideProps, "size" | "className">) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" fillRule="evenodd" className={className} aria-hidden="true">
      <path
        clipRule="evenodd"
        d="M20.998 10.949H24v3.102h-3v3.028h-1.487V20H18v-2.921h-1.487V20H15v-2.921H9V20H7.488v-2.921H6V20H4.487v-2.921H3V14.05H0V10.95h3V5h17.998v5.949zM6 10.949h1.488V8.102H6v2.847zm10.51 0H18V8.102h-1.49v2.847z"
      />
    </svg>
  );
}
