import type { AgentInfo } from "../../lib/assistantAgents";
import AgentIcon from "./AgentIcon";

/** The mark of the agent in use, on a tile, for the screens that stand in for the conversation. */
export default function AgentMark({ agent }: { agent: AgentInfo }) {
  return (
    <span className="flex size-12 items-center justify-center rounded-2xl bg-muted" role="img" aria-label={agent.kind === "custom" ? "Your agent" : agent.name}>
      <AgentIcon kind={agent.kind} size={24} />
    </span>
  );
}
