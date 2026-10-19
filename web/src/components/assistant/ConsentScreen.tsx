import { ShieldCheck } from "lucide-react";
import Button from "../ui/Button";

/** What the assistant sends and keeps, before anything is started. */
export default function ConsentScreen({ onEnable }: { onEnable: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <span className="flex size-11 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
        <ShieldCheck size={20} />
      </span>
      <h2 className="mt-4 text-sm font-semibold">Ask your database</h2>
      <p className="mt-2 max-w-xs text-[13px] leading-relaxed text-muted-foreground">
        The assistant runs Claude Code on this computer with your Claude account. It can read this connection&apos;s schema and run queries that
        only read, after you allow them. Your schema and the results you allow are sent to Anthropic. Claude Code keeps the conversations in
        ~/.claude/projects. You need Node.js 20 or newer and to be signed in to Claude Code.
      </p>
      <Button variant="primary" size="sm" className="mt-5" onClick={onEnable}>
        Enable assistant
      </Button>
    </div>
  );
}
