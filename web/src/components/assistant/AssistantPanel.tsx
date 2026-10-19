import { useState } from "react";
import { useAssistant } from "../../contexts/assistant";
import { clampPanelWidth, DEFAULT_PANEL_WIDTH, PANEL_WIDTH_KEY, storedPanelWidth } from "../../lib/assistantPanelWidth";
import SideCard from "../layout/SideCard";
import AssistantBody from "./AssistantBody";

function remembered(): number {
  try {
    return storedPanelWidth(localStorage.getItem(PANEL_WIDTH_KEY), window.innerWidth);
  } catch {
    return DEFAULT_PANEL_WIDTH;
  }
}

/** The assistant beside the page, opened from the tab bar or with Cmd/Ctrl+I. Its edge can be dragged to change its width. */
export default function AssistantPanel() {
  const assistant = useAssistant();
  const [width, setWidth] = useState(remembered);
  const [resizing, setResizing] = useState(false);

  function startResize(event: React.PointerEvent) {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = width;
    let value = width;
    setResizing(true);
    // The panel is on the right: dragging the edge to the left makes it wider
    const onMove = (move: PointerEvent) => {
      value = clampPanelWidth(startWidth + (startX - move.clientX), window.innerWidth);
      setWidth(value);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setResizing(false);
      try {
        localStorage.setItem(PANEL_WIDTH_KEY, String(value));
      } catch {
        // only for this session
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  function resetWidth() {
    setWidth(DEFAULT_PANEL_WIDTH);
    try {
      localStorage.removeItem(PANEL_WIDTH_KEY);
    } catch {
      // nothing stored
    }
  }

  if (!assistant.available) return null;
  return (
    <SideCard open={assistant.panelOpen && !assistant.inTab} widthClass="w-96" width={width} resizing={resizing} label="Assistant">
      <div className="relative flex h-full min-h-0 flex-col">
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize the assistant"
          onPointerDown={startResize}
          onDoubleClick={resetWidth}
          className="absolute inset-y-0 left-0 z-10 w-2 cursor-col-resize after:absolute after:inset-y-0 after:left-0 after:w-0.5 after:bg-primary/60 after:opacity-0 after:transition-opacity hover:after:opacity-100"
        />
        <AssistantBody onClose={() => assistant.setPanelOpen(false)} />
      </div>
    </SideCard>
  );
}
