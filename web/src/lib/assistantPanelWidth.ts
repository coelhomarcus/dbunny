/** How wide the assistant's side panel is, and the room it leaves for the page beside it. */
export const DEFAULT_PANEL_WIDTH = 384;
export const MIN_PANEL_WIDTH = 320;
const MAX_PANEL_WIDTH = 720;
/** What the editor and the explorer keep when the panel is as wide as it can be. */
const ROOM_FOR_PAGE = 480;

export const PANEL_WIDTH_KEY = "dbunny:assistantPanelWidth";

/** `width` kept between the smallest and the widest the window allows. */
export function clampPanelWidth(width: number, viewport: number): number {
  const widest = Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, viewport - ROOM_FOR_PAGE));
  return Math.round(Math.min(widest, Math.max(MIN_PANEL_WIDTH, width)));
}

/** The width kept from an earlier session (the text in local storage), or the usual one. */
export function storedPanelWidth(stored: string | null, viewport: number): number {
  const width = stored ? Number(stored) : Number.NaN;
  return Number.isFinite(width) ? clampPanelWidth(width, viewport) : DEFAULT_PANEL_WIDTH;
}
