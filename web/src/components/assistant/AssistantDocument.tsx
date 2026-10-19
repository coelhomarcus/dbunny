import { useAssistant } from "../../contexts/assistant";
import AssistantBody from "./AssistantBody";

/** The assistant in a tab of its own: the same conversation as in the side panel, with room to read. */
export default function AssistantDocument() {
  const assistant = useAssistant();
  return (
    <div className="mx-auto h-full max-w-3xl">
      <AssistantBody onBack={assistant.backToPanel} />
    </div>
  );
}
