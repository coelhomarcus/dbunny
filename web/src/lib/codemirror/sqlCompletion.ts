import {
  completeAnyWord,
  completeFromList,
  ifNotIn,
  type Completion,
  type CompletionContext,
  type CompletionSource,
} from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import {
  PostgreSQL,
  keywordCompletionSource,
  schemaCompletionSource,
  type SQLNamespace,
} from "@codemirror/lang-sql";
import type { Extension } from "@codemirror/state";
import type { CompletionRelation, CompletionSchema } from "@/types";

export const sqlDialect = PostgreSQL;

// No completions inside strings, quoted identifiers or comments
const SKIP_NODES = ["String", "QuotedIdentifier", "LineComment", "BlockComment"];

// The dialect only completes single words; these come as one item
const SQL_PHRASES = [
  "INNER JOIN", "LEFT JOIN", "RIGHT JOIN", "FULL JOIN", "CROSS JOIN",
  "IS NULL", "IS NOT NULL", "NOT NULL", "ORDER BY", "GROUP BY",
  "UNION ALL", "PRIMARY KEY", "FOREIGN KEY", "NULLS FIRST", "NULLS LAST",
  "INSERT INTO", "DELETE FROM", "ON CONFLICT", "DO NOTHING", "IF EXISTS",
  "IF NOT EXISTS",
];

const SQL_WORDS = new Set(
  [sqlDialect.spec.keywords, sqlDialect.spec.types, sqlDialect.spec.builtin]
    .join(" ")
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean),
);

// Words that must be quoted when used as identifiers in PostgreSQL
const RESERVED = new Set(
  ("all analyse analyze and any array as asc asymmetric both case cast check collate column " +
    "constraint create current_catalog current_date current_role current_time current_timestamp " +
    "current_user default deferrable desc distinct do else end except false fetch for foreign from " +
    "grant group having in initially intersect into lateral leading limit localtime localtimestamp " +
    "not null offset on only or order placing primary references returning select session_user " +
    "some symmetric system_user table then to trailing true union unique user using variadic when " +
    "where window with").split(" "),
);

/** Quotes an identifier the way PostgreSQL needs it (mixed case, special chars, reserved words). */
export function quoteIdent(name: string) {
  if (/^[a-z_][a-z0-9_$]*$/.test(name) && !RESERVED.has(name)) return name;
  return `"${name.replace(/"/g, '""')}"`;
}

function applyFor(name: string) {
  const quoted = quoteIdent(name);
  return quoted === name ? undefined : quoted;
}

// lang-sql splits namespace keys on "." unless escaped
function nsKey(name: string) {
  return name.replace(/\./g, "\\.");
}

function buildNamespace(schema: CompletionSchema): SQLNamespace {
  const ns: Record<string, { self: Completion; children: Record<string, SQLNamespace> }> = {};
  const schemaLevel = (name: string) =>
    (ns[nsKey(name)] ??= {
      self: { label: name, type: "namespace", detail: "schema", apply: applyFor(name) },
      children: {},
    }).children;

  for (const rel of schema.relations) {
    schemaLevel(rel.schema)[nsKey(rel.name)] = {
      self: { label: rel.name, type: "type", detail: rel.type, apply: applyFor(rel.name) },
      children: rel.columns.map((c) => ({
        label: c.name,
        type: "property",
        detail: c.dataType,
        apply: applyFor(c.name),
      })),
    };
  }

  for (const fn of schema.functions) {
    const level = schemaLevel(fn.schema);
    const key = nsKey(fn.name);
    if (level[key]) continue; // overloads / name clash with a table: keep the first
    level[key] = {
      self: {
        label: fn.name,
        type: "function",
        detail: `(${fn.argumentTypes})${fn.returnType ? ` → ${fn.returnType}` : ""}`,
        apply: applyFor(fn.name),
      },
      children: [],
    };
  }

  return ns;
}

type RelationIndex = Map<string, CompletionRelation>;

function indexRelations(schema: CompletionSchema): RelationIndex {
  const index: RelationIndex = new Map();
  const current = schema.currentSchema ?? "public";
  for (const rel of schema.relations) {
    index.set(`${rel.schema}.${rel.name}`, rel);
    // Unqualified names resolve to the current schema first, then anywhere
    const existing = index.get(rel.name);
    if (!existing || (rel.schema === current && existing.schema !== current)) {
      index.set(rel.name, rel);
    }
  }
  return index;
}

const IDENT = String.raw`(?:"(?:[^"]|"")+"|[A-Za-z_][\w$]*)`;
const IDENT_RE = new RegExp(IDENT, "g");
const TABLE_REF = new RegExp(String.raw`(?:\b(?:from|join|update|into|table)\s+|,\s*)(${IDENT}(?:\s*\.\s*${IDENT})?)`, "gi");

function unquote(part: string) {
  return part.startsWith('"') ? part.slice(1, -1).replace(/""/g, '"') : part.toLowerCase();
}

function statementText(context: CompletionContext) {
  let node = syntaxTree(context.state).resolveInner(context.pos, -1);
  while (node.name !== "Statement" && node.parent) node = node.parent;
  if (node.name === "Statement") return context.state.sliceDoc(node.from, node.to);
  return context.state.doc.toString();
}

/**
 * Columns of the tables used in the current statement (FROM / JOIN / UPDATE / INTO),
 * offered without needing a `table.` prefix.
 */
function columnsInScope(index: RelationIndex): CompletionSource {
  return (context) => {
    const word = context.matchBefore(/[\w$]*/);
    if (!word || (word.from === word.to && !context.explicit)) return null;
    if (context.state.sliceDoc(word.from - 1, word.from) === ".") return null;

    const seen = new Set<CompletionRelation>();
    for (const match of statementText(context).matchAll(TABLE_REF)) {
      const path = (match[1].match(IDENT_RE) ?? []).map(unquote);
      const rel = index.get(path.join("."));
      if (rel) seen.add(rel);
    }
    if (seen.size === 0) return null;

    const options: Completion[] = [];
    for (const rel of seen) {
      for (const col of rel.columns) {
        options.push({
          label: col.name,
          type: "property",
          detail: seen.size > 1 ? `${rel.name} · ${col.dataType}` : col.dataType,
          apply: applyFor(col.name),
          boost: 2,
        });
      }
    }
    return { from: word.from, options, validFor: /^[\w$]*$/ };
  };
}

/**
 * lang-sql treats the word being typed after `FROM table` as an alias of that
 * table and suggests it back; drop that self-suggestion.
 */
function withoutSelfAlias(source: CompletionSource): CompletionSource {
  return async (context) => {
    const result = await source(context);
    if (!result) return null;
    const typed = context.state.sliceDoc(result.from, context.pos);
    return { ...result, options: result.options.filter((o) => !(o.type === "constant" && o.label === typed)) };
  };
}

/** Words already typed in the document (CTE names, aliases…) that aren't keywords or schema objects. */
function documentWords(known: Set<string>): CompletionSource {
  return async (context) => {
    const result = await completeAnyWord(context);
    if (!result) return null;
    const options = result.options.filter((o) => {
      const word = o.label.toLowerCase();
      return !SQL_WORDS.has(word) && !known.has(word);
    });
    return options.length ? { ...result, options } : null;
  };
}

const phraseSource = ifNotIn(
  SKIP_NODES,
  completeFromList(SQL_PHRASES.map((label) => ({ label, type: "keyword", boost: -1 }))),
);

/**
 * All autocomplete sources for the SQL editor. `schema` is null until the
 * catalog has loaded, in which case only keywords and document words are offered.
 */
export function sqlCompletion(schema: CompletionSchema | null): Extension {
  const known = new Set<string>();
  const sources: CompletionSource[] = [
    keywordCompletionSource(sqlDialect, true),
    phraseSource,
  ];

  if (schema) {
    for (const rel of schema.relations) {
      known.add(rel.schema.toLowerCase());
      known.add(rel.name.toLowerCase());
      for (const col of rel.columns) known.add(col.name.toLowerCase());
    }
    for (const fn of schema.functions) known.add(fn.name.toLowerCase());

    sources.push(
      withoutSelfAlias(
        schemaCompletionSource({
          dialect: sqlDialect,
          schema: buildNamespace(schema),
          defaultSchema: schema.currentSchema ?? "public",
        }),
      ),
      ifNotIn(SKIP_NODES, columnsInScope(indexRelations(schema))),
    );
  }

  sources.push(ifNotIn(SKIP_NODES, documentWords(known)));

  return sources.map((autocomplete) => sqlDialect.language.data.of({ autocomplete }));
}
