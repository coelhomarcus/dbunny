/** Where the database context starts in a message: the assistant hands it to agents that take no system prompt. */
export const CONTEXT_OPEN = "<dbunny-context>";
const CONTEXT_CLOSE = "</dbunny-context>";

/** The message without the database context, which is for the agent and not for the person to read. */
export function stripContext(text: string): string {
  let out = text;
  for (let start = out.indexOf(CONTEXT_OPEN); start >= 0; start = out.indexOf(CONTEXT_OPEN)) {
    const end = out.indexOf(CONTEXT_CLOSE, start);
    // A block still arriving has no end yet: it goes to the end of the text
    const after = end < 0 ? "" : out.slice(end + CONTEXT_CLOSE.length);
    out = out.slice(0, start).trimEnd() + (after.trim() === "" ? "" : "\n\n" + after.trimStart());
  }
  return out;
}
