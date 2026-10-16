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
    <div className="mr-1 flex items-center gap-1.5">
      <Tooltip label="Rollback undoes every change since the transaction began" side="top">
        <Button variant="outline" size="sm" onClick={onRollback} disabled={disabled}>
          <Undo2 />
          Rollback
        </Button>
      </Tooltip>
      <Tooltip label="A transaction is open. Commit keeps its changes" side="top">
        <Button variant="success" size="sm" onClick={onCommit} disabled={disabled}>
          <Check />
          Commit
        </Button>
      </Tooltip>
    </div>
  );
}
