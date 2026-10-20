import { Check, GitCommitVertical, Undo2 } from "lucide-react";
import Button from "../ui/Button";
import Tooltip from "../Tooltip";
import { useT } from "../../i18n";

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
  const t = useT();
  if (!open) {
    return (
      <Tooltip label={t("query.transactionButton.beginATransactionRunsJoin")} side="top">
        <Button variant="ghost-muted" size="sm" onClick={onBegin} disabled={disabled}>
          <GitCommitVertical />
          {t("query.transactionButton.transaction")}
        </Button>
      </Tooltip>
    );
  }
  return (
    <div className="mr-1 flex items-center gap-2">
      {/* Says a transaction is open; the buttons beside it end it */}
      <Tooltip label={t("query.transactionButton.transactionOpenRunsJoinIt")} side="top">
        <span role="status" aria-label={t("query.transactionButton.inATransaction")} className="flex size-7 items-center justify-center text-primary">
          <GitCommitVertical size={16} />
        </span>
      </Tooltip>
      {/* Joined like Run, each half in its own colour */}
      <div className="flex">
        <Tooltip label={t("query.transactionButton.undoTheTransactionsChanges")} side="top">
          <Button variant="destructive" size="sm" className="rounded-r-none" onClick={onRollback} disabled={disabled}>
            <Undo2 />
            {t("query.transactionButton.rollback")}
          </Button>
        </Tooltip>
        <Tooltip label={t("query.transactionButton.keepTheTransactionsChanges")} side="top">
          <Button variant="success" size="sm" className="rounded-l-none" onClick={onCommit} disabled={disabled}>
            <Check />
            {t("query.transactionButton.commit")}
          </Button>
        </Tooltip>
      </div>
    </div>
  );
}
