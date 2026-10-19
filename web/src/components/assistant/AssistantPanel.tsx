import { useAssistant } from "../../contexts/assistant";
import SideCard from "../layout/SideCard";
import AssistantBody from "./AssistantBody";

/** The assistant beside the page, opened from the tab bar or with Cmd/Ctrl+I. */
export default function AssistantPanel() {
  const assistant = useAssistant();
  if (!assistant.available) return null;
  return (
    <SideCard open={assistant.panelOpen && !assistant.inTab} widthClass="w-96" label="Assistant">
      <AssistantBody onClose={() => assistant.setPanelOpen(false)} />
    </SideCard>
  );
}
