import type { Decision } from "../../types/assistant";
import Button from "../ui/Button";

/** What the person may answer when the assistant wants to run a query. */
export default function ApprovalButtons({ onDecide }: { onDecide: (decision: Decision) => void }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      <Button variant="primary" size="xs" onClick={() => onDecide("once")}>
        Allow once
      </Button>
      <Button variant="outline" size="xs" onClick={() => onDecide("readsInChat")}>
        Allow reads in this chat
      </Button>
      <Button variant="ghost-muted" size="xs" onClick={() => onDecide("decline")}>
        Decline
      </Button>
    </div>
  );
}
