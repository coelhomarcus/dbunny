import { createContext, useContext } from "react";

/**
 * Element next to the page card where a page can put a side panel (the query
 * library, say), so it shows as a card of its own. Null until it's mounted.
 */
export const RightPanelContext = createContext<HTMLElement | null>(null);

export function useRightPanel() {
  return useContext(RightPanelContext);
}
