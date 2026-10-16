import { Check, GitCommitVertical, Undo2 } from "lucide-react";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";

/** Begins a transaction; while one is open, offers to end it: Commit, then Rollback. */
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
    <div className="mr-1 flex items-center gap-2">
      {/* Says a transaction is open; the buttons beside it end it */}
      <Tooltip label="Transaction open: runs join it" side="top">
        <span role="status" aria-label="In a transaction" className="flex size-7 items-center justify-center text-primary">
          <GitCommitVertical size={16} />
        </span>
      </Tooltip>
      {/* Joined like Run, each half in its own colour */}
      <div className="flex">
        <Tooltip label="Undo the transaction's changes" side="top">
          <Button variant="destructive" size="sm" className="rounded-r-none" onClick={onRollback} disabled={disabled}>
            <Undo2 />
            Rollback
          </Button>
        </Tooltip>
        <Tooltip label="Keep the transaction's changes" side="top">
          <Button variant="success" size="sm" className="rounded-l-none" onClick={onCommit} disabled={disabled}>
            <Check />
            Commit
          </Button>
        </Tooltip>
      </div>
    </div>
  );
}
