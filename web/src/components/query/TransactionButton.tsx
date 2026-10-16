import { Check, GitCommitVertical, Undo2 } from "lucide-react";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";

/**
 * Begins a transaction; while one is open, becomes a split accent button (like Run) in the
 * warning colour, to end it: Commit on the left, Rollback on the right.
 */
export default function TransactionButton({
  open,
  disabled,
  onBegin,
  onCommit,
  onRollback,
}: {
  open: boolean;
  disabled: boolean;
  onBegin: () => void;
  onCommit: () => void;
  onRollback: () => void;
}) {
  if (!open) {
    return (
      <Tooltip label="Begin a transaction: runs join it until commit or rollback" side="top">
        <Button variant="ghost-muted" size="sm" onClick={onBegin} disabled={disabled}>
          <GitCommitVertical />
          Transaction
        </Button>
      </Tooltip>
    );
  }
  return (
    <div className="mr-1 flex">
      <Tooltip label="A transaction is open. Commit keeps its changes" side="top">
        <Button variant="warning" size="sm" className="rounded-r-none" onClick={onCommit} disabled={disabled}>
          <Check />
          Commit
        </Button>
      </Tooltip>
      <Tooltip label="Rollback undoes every change since it began" side="top">
        <Button variant="warning" size="sm" className="rounded-l-none border-l-zinc-950/20" onClick={onRollback} disabled={disabled}>
          <Undo2 />
          Rollback
        </Button>
      </Tooltip>
    </div>
  );
}
