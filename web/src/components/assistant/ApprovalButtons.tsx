import type { Decision } from "../../types/assistant";
import Button from "../ui/Button";
import { useT } from "../../i18n";

/** What the person may answer when the assistant wants to run a query. */
export default function ApprovalButtons({ onDecide }: { onDecide: (decision: Decision) => void }) {
  const t = useT();
  return (
    <div className="flex flex-wrap gap-1.5">
      <Button variant="primary" size="xs" onClick={() => onDecide("once")}>
        {t("assistant.approvalButtons.allowOnce")}
      </Button>
      <Button variant="outline" size="xs" onClick={() => onDecide("readsInChat")}>
        {t("assistant.approvalButtons.allowReadsInThisChat")}
      </Button>
      <Button variant="ghost-muted" size="xs" onClick={() => onDecide("decline")}>
        {t("assistant.approvalButtons.decline")}
      </Button>
    </div>
  );
}
