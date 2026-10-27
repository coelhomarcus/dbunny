import { completeAnyWord, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import type { CompletionSchema, Engine, SqlFeature } from "@/types";
import { dialectOf } from "../dialects/index.ts";
import { analyze, type Position } from "./context.ts";
import { iconSvg, isItemKind } from "./icons.ts";
import { insideQuotes, optionsAt } from "./sources.ts";

/** Positions that open the list by themselves, before anything is typed. */
const OPENS_BY_ITSELF = new Set<Position["kind"]>(["table", "column", "joinCondition", "qualified", "insertColumns"]);
const WORD = /^[\w$]*$/;

/** The item's icon, before its label; drawn by `autocompletion({ addToOptions })`. */
export const completionIcon = {
  render(completion: Completion) {
    if (!isItemKind(completion.type)) return null;
    const icon = document.createElement("span");
    icon.className = `cm-sqlIcon cm-sqlIcon-${completion.type}`;
    icon.setAttribute("aria-hidden", "true");
    icon.innerHTML = iconSvg(completion.type);
    return icon;
  },
  position: 20,
};

/** Words of the document no other suggestion has: names the query makes up as it goes. */
async function documentWords(context: CompletionContext, known: Set<string>): Promise<Completion[]> {
  const result = (await completeAnyWord(context)) as CompletionResult | null;
  if (!result) return [];
  return result.options
    .filter((o) => !known.has(o.label.toLowerCase()))
    .map((o) => ({ label: o.label, type: "local", boost: -20 }));
}

/**
 * The SQL editor's suggestions for `engine`: what the query around the cursor asks for, from the
 * connected database's objects and its dialect's words and functions. `schema` is null until the
 * catalog loads; the dialect's words and functions are offered meanwhile.
 */
export function sqlCompletion(schema: CompletionSchema | null, engine: Engine, features: SqlFeature[]): Extension {
  const dialect = dialectOf(engine);
  const autocomplete = async (context: CompletionContext): Promise<CompletionResult | null> => {
    const ctx = analyze(context.state.doc.toString(), context.pos, dialect, features);
    const { position, word } = ctx;
    if (position.kind === "quiet") return null;
    if (!word.text && !context.explicit) {
      if (!OPENS_BY_ITSELF.has(position.kind)) return null;
      // `SELECT |` before any FROM would only list every function
      if (position.kind === "column" && ctx.relations.length === 0) return null;
    }
    let options = optionsAt(schema, ctx, dialect, features);
    const quote = word.text[0];
    if (quote && /["`[]/.test(quote)) {
      // Inside a quoted name: match the name itself, and write it without the quotes already typed
      const close = quote === "[" ? "]" : quote;
      options = insideQuotes(options, close);
      return { from: word.from + 1, options, validFor: new RegExp(`^[^${close === "]" ? "\\]" : close}]*$`) };
    }
    if (word.text && position.kind !== "qualified" && position.kind !== "insertColumns") {
      const known = new Set(options.map((o) => o.label.toLowerCase()));
      options = [...options, ...(await documentWords(context, known))];
    }
    return { from: word.from, options, validFor: WORD };
  };
  return dialect.language.language.data.of({ autocomplete });
}
