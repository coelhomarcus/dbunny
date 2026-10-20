import type { AgentInfo } from "../../lib/assistantAgents";
import AgentIcon from "./AgentIcon";

/** The mark of the agent in use, large and on its own, for the screens that stand in for the conversation. */
export default function AgentMark({ agent }: { agent: AgentInfo }) {
  return (
    <span role="img" aria-label={agent.kind === "custom" ? "Your agent" : agent.name}>
      {/* Claude's mark keeps its own color; the others take the text color, which reads on a light and on a dark window */}
      <AgentIcon kind={agent.kind} size={48} className="text-foreground" />
    </span>
  );
}
