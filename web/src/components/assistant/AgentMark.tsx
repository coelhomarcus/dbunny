import type { AgentInfo } from "../../lib/assistantAgents";
import AgentIcon from "./AgentIcon";
import { useT } from "../../i18n";

/** The mark of the agent in use, large and on its own, for the screens that stand in for the conversation. */
export default function AgentMark({ agent }: { agent: AgentInfo }) {
  const t = useT();
  return (
    <span role="img" aria-label={agent.kind === "custom" ? t("assistant.agentMark.yourAgent") : agent.name}>
      {/* Claude's mark keeps its own color; the others take the text color, which reads on a light and on a dark window */}
      <AgentIcon kind={agent.kind} size={48} className="text-foreground" />
    </span>
  );
}
