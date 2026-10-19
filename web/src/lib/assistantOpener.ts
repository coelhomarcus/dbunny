import { applyUpdate, emptyConversation, type Conversation } from "./assistantConversation.ts";
import type { OpenedConversation } from "../types/assistant";

/**
 * Opens conversations one after another in effect: the newest opening wins. An older one that
 * ends later is closed and gives nothing, so a chat made ahead of the first message can't take
 * the place of a past one the person picked meanwhile.
 *
 * A reopened conversation replays its messages before its id is known; the updates that arrive
 * meanwhile are kept by conversation until the opening ends.
 */
export class ConversationOpener {
  private generation = 0;
  private active = 0;
  private buffered = new Map<string, unknown[]>();

  private readonly close: (conversationId: string) => void;

  constructor(close: (conversationId: string) => void) {
    this.close = close;
  }

  /** Whether a conversation is opening, so that its updates are kept. */
  get opening(): boolean {
    return this.active > 0;
  }

  /** Whatever is opening is no longer wanted. */
  invalidate(): void {
    this.generation++;
  }

  /** Keeps an update of a conversation whose opening is not over; false when none is opening. */
  receive(conversationId: string, update: unknown): boolean {
    if (this.active === 0) return false;
    this.buffered.set(conversationId, [...(this.buffered.get(conversationId) ?? []), update]);
    return true;
  }

  /** What `run` opens with its replay applied, or null when a newer opening or `invalidate` came first. */
  async open(run: () => Promise<OpenedConversation>): Promise<{ opened: OpenedConversation; state: Conversation } | null> {
    const mine = ++this.generation;
    this.active++;
    try {
      const opened = await run();
      const updates = this.buffered.get(opened.conversationId) ?? [];
      this.buffered.delete(opened.conversationId);
      if (mine !== this.generation) {
        this.close(opened.conversationId);
        return null;
      }
      let state = emptyConversation();
      for (const update of updates) state = applyUpdate(state, update);
      return { opened, state };
    } catch (error) {
      if (mine !== this.generation) return null;
      throw error;
    } finally {
      this.active--;
      if (this.active === 0) this.buffered.clear();
    }
  }
}
