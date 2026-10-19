import {
  completeAnyWord,
  completeFromList,
  ifNotIn,
  type Completion,
  type CompletionContext,
  type CompletionSource,
} from "@codemirror/autocomplete";
import {
  MySQL,
  PostgreSQL,
  SQLite,
  keywordCompletionSource,
  schemaCompletionSource,
  type SQLNamespace,
} from "@codemirror/lang-sql";
import type { Extension } from "@codemirror/state";
import type { CompletionRelation, CompletionSchema } from "@/types";
import type { SqlDialectName } from "../sqlStatements";

export const sqlDialect = PostgreSQL;

/** The editor's language for a database's SQL. */
export function dialectFor(name: SqlDialectName) {
  if (name === "sqlite") return SQLite;
  return name === "mysql" ? MySQL : PostgreSQL;
}

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

function reservedWords(dialect: SqlDialectName): Set<string> {
  const spec = dialectFor(dialect).spec;
  return new Set(
    [spec.keywords, spec.types, spec.builtin]
      .join(" ")
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean),
  );
}

/** Quotes an identifier for the active SQL dialect (reserved words, case, special chars). */
export function quoteIdentForDialect(name: string, dialect: SqlDialectName) {
  const reserved = reservedWords(dialect);
  const word = name.toLowerCase();
  if (/^[a-z_][a-z0-9_$]*$/.test(name) && !reserved.has(word)) return name;
  // MySQL reads double quotes as strings: its names go in backticks
  if (dialect === "mysql") return `\`${name.replace(/`/g, "``")}\``;
  return `"${name.replace(/"/g, '""')}"`;
}

/** PostgreSQL quoting for copy/export helpers that aren't tied to the editor dialect. */
export function quoteIdent(name: string) {
  return quoteIdentForDialect(name, "postgres");
}

function applyFor(name: string, quoteIdent: (n: string) => string) {
  const quoted = quoteIdent(name);
  return quoted === name ? undefined : quoted;
}

// lang-sql splits namespace keys on "." unless escaped
function nsKey(name: string) {
  return name.replace(/\./g, "\\.");
}

function buildNamespace(schema: CompletionSchema, quoteIdent: (n: string) => string): SQLNamespace {
  const ns: Record<string, { self: Completion; children: Record<string, SQLNamespace> }> = {};
  const schemaLevel = (name: string) =>
    (ns[nsKey(name)] ??= {
      self: { label: name, type: "namespace", detail: "schema", apply: applyFor(name, quoteIdent) },
      children: {},
    }).children;

  for (const rel of schema.relations) {
    schemaLevel(rel.schema)[nsKey(rel.name)] = {
      self: { label: rel.name, type: "type", detail: rel.type, apply: applyFor(rel.name, quoteIdent) },
      children: rel.columns.map((c) => ({
        label: c.name,
        type: "property",
        detail: c.dataType,
        apply: applyFor(c.name, quoteIdent),
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
        apply: applyFor(fn.name, quoteIdent),
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
const QUALIFIED = String.raw`(${IDENT}(?:\s*\.\s*${IDENT})?)`;
// Two separate passes: a single regex would let `, FROM` swallow the `FROM users` after it
const TABLE_REFS = [
  new RegExp(String.raw`\b(?:from|join|update|into|table)\s+${QUALIFIED}`, "gi"),
  new RegExp(String.raw`,\s*${QUALIFIED}`, "gi"),
];

function unquote(part: string) {
  return part.startsWith('"') ? part.slice(1, -1).replace(/""/g, '"') : part.toLowerCase();
}

/** Bounds of the statement around `pos`, split on `;`. */
function statementRange(context: CompletionContext, pos: number) {
  const doc = context.state.doc.toString();
  const end = doc.indexOf(";", pos);
  return { from: doc.lastIndexOf(";", pos - 1) + 1, to: end < 0 ? doc.length : end };
}

function statementText(context: CompletionContext) {
  const { from, to } = statementRange(context, context.pos);
  return context.state.sliceDoc(from, to);
}

/** Statement text before `pos`, with comments and string contents blanked out. */
function textBefore(context: CompletionContext, pos: number) {
  return context.state
    .sliceDoc(statementRange(context, pos).from, pos)
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/'(?:[^']|'')*'/g, "''");
}

const AFTER_TABLE_KEYWORD = /\b(?:from|join|update|into|table|truncate)\s+$/i;
const AFTER_COLUMN_KEYWORD = /\b(?:select|distinct|where|and|or|on|set|by|having|returning)\s+$/i;
const CLAUSE_KEYWORD = /\b(select|from|where|group\s+by|order\s+by|having|set|returning|on|into|values|limit)\b/gi;
const COLUMN_LIST_CLAUSES = new Set(["select", "where", "group by", "order by", "having", "set", "returning", "on", "into"]);

/**
 * What belongs at an empty position: a table (`FROM |`, `JOIN |`, `FROM a, |`)
 * or a column (`SELECT |`, `WHERE |`, `ORDER BY a, |`, `INSERT INTO t (|`).
 */
function positionKind(before: string): "table" | "column" | null {
  if (AFTER_TABLE_KEYWORD.test(before)) return "table";
  if (AFTER_COLUMN_KEYWORD.test(before)) return "column";

  const afterComma = /,\s*$/.test(before);
  const afterParen = /\(\s*$/.test(before);
  if (!afterComma && !afterParen) return null;

  const clause = [...before.matchAll(CLAUSE_KEYWORD)].at(-1)?.[1].toLowerCase().replace(/\s+/g, " ");
  if (!clause) return null;
  if (afterComma && clause === "from") return "table";
  // `INSERT INTO t (` opens the column list; elsewhere a paren is a function call or subquery
  if (afterParen) return clause === "into" ? "column" : null;
  return COLUMN_LIST_CLAUSES.has(clause) ? "column" : null;
}

/**
 * Columns of the tables used in the current statement (FROM / JOIN / UPDATE / INTO),
 * offered without needing a `table.` prefix.
 */
function columnsInScope(index: RelationIndex, quoteIdent: (n: string) => string): CompletionSource {
  return (context) => {
    const word = context.matchBefore(/[\w$]*/);
    if (!word || context.state.sliceDoc(word.from - 1, word.from) === ".") return null;
    // With nothing typed yet, only open by itself where a column is expected
    if (word.from === word.to && !context.explicit && positionKind(textBefore(context, word.from)) !== "column") {
      return null;
    }

    const seen = new Set<CompletionRelation>();
    const text = statementText(context);
    for (const match of TABLE_REFS.flatMap((re) => [...text.matchAll(re)])) {
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
          apply: applyFor(col.name, quoteIdent),
          boost: 2,
        });
      }
    }
    return { from: word.from, options, validFor: /^[\w$]*$/ };
  };
}

/**
 * Opens the table list right after `FROM `, `JOIN `, `UPDATE `… without typing
 * anything. Once a word is typed, lang-sql's schema source takes over; options
 * here match its fields exactly so the two don't show duplicates.
 */
function tablesAfterKeyword(schema: CompletionSchema, quoteIdent: (n: string) => string): CompletionSource {
  const current = schema.currentSchema ?? "public";
  const options: Completion[] = schema.relations.map((rel) => {
    if (rel.schema === current) {
      return { label: rel.name, type: "type", detail: rel.type, apply: applyFor(rel.name, quoteIdent) };
    }
    const qualified = `${rel.schema}.${rel.name}`;
    const applied = `${quoteIdent(rel.schema)}.${quoteIdent(rel.name)}`;
    return { label: qualified, type: "type", detail: rel.type, apply: applied === qualified ? undefined : applied };
  });

  return (context) => {
    const word = context.matchBefore(/[\w$]*/);
    if (!word || word.from !== word.to) return null;
    if (positionKind(textBefore(context, word.from)) !== "table") return null;
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
export function sqlCompletion(schema: CompletionSchema | null, name: SqlDialectName = "postgres"): Extension {
  const dialect = dialectFor(name);
  const quoteIdent = (ident: string) => quoteIdentForDialect(ident, name);
  const known = new Set<string>();
  const sources: CompletionSource[] = [
    keywordCompletionSource(dialect, true),
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
          dialect,
          schema: buildNamespace(schema, quoteIdent),
          defaultSchema: schema.currentSchema ?? "public",
        }),
      ),
      ifNotIn(SKIP_NODES, columnsInScope(indexRelations(schema), quoteIdent)),
      ifNotIn(SKIP_NODES, tablesAfterKeyword(schema, quoteIdent)),
    );
  }

  sources.push(ifNotIn(SKIP_NODES, documentWords(known)));

  return sources.map((autocomplete) => dialect.language.data.of({ autocomplete }));
}
