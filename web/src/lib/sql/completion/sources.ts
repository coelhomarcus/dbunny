import { snippetCompletion, type Completion } from "@codemirror/autocomplete";
import type { CompletionColumn, CompletionRelation, CompletionSchema, SqlFeature } from "@/types";
import { t, tr } from "../../../i18n/store.ts";
import { available, functionKey } from "../dialects/index.ts";
import type { SqlDialect } from "../dialects/types.ts";
import type { Position, QueryContext, QueryRelation } from "./context.ts";
import { ICON_LABEL, type ItemKind } from "./icons.ts";

/** How high each kind of suggestion ranks: what the position asks for, then objects, functions, words. */
const BOOST = {
  joinCondition: 60,
  related: 55,
  cte: 52,
  wanted: 50,
  alias: 45,
  schema: 40,
  catalogFunction: 20,
  userFunction: 18,
  serverFunction: 10,
  snippet: 0,
  phrase: -8,
  keyword: -10,
  dataType: -12,
};

const label = (kind: ItemKind) => t(ICON_LABEL[kind]);

/** The name as the dialect has to write it; undefined when it reads the same. */
function applyName(name: string, dialect: SqlDialect) {
  const quoted = dialect.quoteIdent(name);
  return quoted === name ? undefined : quoted;
}

interface SchemaIndex {
  byName: Map<string, CompletionRelation[]>;
  schemas: string[];
}

const indexes = new WeakMap<CompletionSchema, SchemaIndex>();

function indexOf(schema: CompletionSchema): SchemaIndex {
  let index = indexes.get(schema);
  if (!index) {
    const byName = new Map<string, CompletionRelation[]>();
    const schemas = new Set<string>();
    for (const relation of schema.relations) {
      const key = relation.name.toLowerCase();
      byName.set(key, [...(byName.get(key) ?? []), relation]);
      schemas.add(relation.schema);
    }
    for (const fn of schema.functions) schemas.add(fn.schema);
    index = { byName, schemas: [...schemas] };
    indexes.set(schema, index);
  }
  return index;
}

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The table or view a name in the query means: its schema if written, else the current one, else any. */
function resolve(schema: CompletionSchema, relation: Pick<QueryRelation, "schema" | "name">): CompletionRelation | undefined {
  const candidates = (indexOf(schema).byName.get(relation.name.toLowerCase()) ?? []).filter((r) => !relation.schema || same(r.schema, relation.schema));
  return (
    candidates.find((r) => r.name === relation.name && r.schema === (relation.schema ?? schema.currentSchema)) ??
    candidates.find((r) => r.schema === schema.currentSchema) ??
    candidates.find((r) => r.name === relation.name) ??
    candidates[0]
  );
}

function columnKind(column: CompletionColumn): ItemKind {
  if (column.primaryKey) return "columnPrimaryKey";
  return column.references ? "columnForeignKey" : "column";
}

function columnOption(column: CompletionColumn, dialect: SqlDialect, owner: string | null, boost = BOOST.wanted): Completion {
  const flags = [column.dataType];
  if (column.primaryKey) flags.push(t("ui.sqlCompletion.primaryKey"));
  if (column.references) flags.push(t("ui.sqlCompletion.foreignKey", { table: column.references.table }));
  const info = [column.dataType, t(column.nullable ? "ui.sqlCompletion.nullable" : "ui.sqlCompletion.notNull")];
  if (column.references) {
    const { schema, table, column: target } = column.references;
    info.push(t("ui.sqlCompletion.references", { target: `${schema}.${table}.${target}` }));
  }
  return {
    label: column.name,
    type: columnKind(column),
    detail: (owner ? [owner, ...flags] : flags).filter(Boolean).join(" · "),
    info: info.filter(Boolean).join("\n"),
    apply: applyName(column.name, dialect),
    boost,
  };
}

function cteColumns(name: string, columns: string[], dialect: SqlDialect, owner: string | null): Completion[] {
  return columns.map((column) => ({ label: column, type: "column", detail: owner ?? name, apply: applyName(column, dialect), boost: BOOST.wanted }));
}

/** The columns a relation of the query has: a CTE's declared ones, or the table's. */
function columnsOf(schema: CompletionSchema | null, ctx: QueryContext, relation: QueryRelation, dialect: SqlDialect, owner: string | null) {
  const cte = relation.schema ? undefined : ctx.ctes.find((c) => same(c.name, relation.name));
  if (cte) return cteColumns(cte.name, cte.columns, dialect, owner);
  const known = schema && resolve(schema, relation);
  return known ? known.columns.map((column) => columnOption(column, dialect, owner)) : [];
}

/** Tables, views and CTEs to read from; after JOIN, the ones tied by a foreign key come first. */
export function relationOptions(schema: CompletionSchema | null, ctx: QueryContext, dialect: SqlDialect): Completion[] {
  const options: Completion[] = ctx.ctes.map((cte) => ({ label: cte.name, type: "local", detail: label("local"), apply: applyName(cte.name, dialect), boost: BOOST.cte }));
  if (!schema) return options;
  const related = ctx.position.kind === "table" && ctx.position.afterJoin ? relatedTo(schema, ctx.relations) : new Set<CompletionRelation>();
  for (const relation of schema.relations) {
    const current = relation.schema === schema.currentSchema;
    const name = current ? relation.name : `${relation.schema}.${relation.name}`;
    const quoted = current ? dialect.quoteIdent(relation.name) : `${dialect.quoteIdent(relation.schema)}.${dialect.quoteIdent(relation.name)}`;
    options.push({
      label: name,
      type: relation.type,
      detail: label(relation.type),
      apply: quoted === name ? undefined : quoted,
      boost: related.has(relation) ? BOOST.related : BOOST.wanted,
    });
  }
  for (const name of indexOf(schema).schemas) {
    if (name !== schema.currentSchema) options.push({ label: name, type: "schema", detail: label("schema"), apply: applyName(name, dialect), boost: BOOST.schema });
  }
  return options;
}

/** The tables a foreign key ties to any relation of the query, either way. */
function relatedTo(schema: CompletionSchema, relations: QueryRelation[]) {
  const present = new Set(relations.map((r) => resolve(schema, r)).filter((r): r is CompletionRelation => !!r));
  const related = new Set<CompletionRelation>();
  for (const relation of present) {
    for (const column of relation.columns) {
      const target = column.references && resolve(schema, { schema: column.references.schema, name: column.references.table });
      if (target) related.add(target);
    }
  }
  for (const relation of schema.relations) {
    if (relation.columns.some((c) => c.references && [...present].some((p) => same(p.name, c.references!.table) && same(p.schema, c.references!.schema)))) related.add(relation);
  }
  for (const relation of present) related.delete(relation);
  return related;
}


/** The columns of the relations the query reads, and their aliases. */
export function columnOptions(schema: CompletionSchema | null, ctx: QueryContext, dialect: SqlDialect): Completion[] {
  const many = ctx.relations.length > 1;
  const options: Completion[] = [];
  for (const relation of ctx.relations) {
    options.push(...columnsOf(schema, ctx, relation, dialect, many ? (relation.alias ?? relation.name) : null));
    // Written back as the query has it: an unquoted `Users` is `users` to PostgreSQL, `"Users"` isn't
    const name = relation.alias ?? relation.name;
    options.push({ label: name, type: "local", detail: label("local"), apply: relation.written === name ? undefined : relation.written, boost: BOOST.alias });
  }
  return options;
}

/** `<joined>.<column> = <other>.<column>` for each foreign key between the joined relation and the others. */
export function joinConditionOptions(schema: CompletionSchema | null, ctx: QueryContext, dialect: SqlDialect): Completion[] {
  if (!schema || ctx.position.kind !== "joinCondition") return [];
  const target = ctx.position.target;
  const joined = resolve(schema, target);
  if (!joined) return [];
  const q = (name: string) => dialect.quoteIdent(name);
  const points = (column: CompletionColumn, at: CompletionRelation) =>
    !!column.references && same(column.references.table, at.name) && same(column.references.schema, at.schema);
  const conditions: string[] = [];
  for (const other of ctx.relations) {
    if (other === target) continue;
    const known = resolve(schema, other);
    if (!known) continue;
    const [left, right] = [target.written, other.written];
    for (const column of joined.columns) {
      if (points(column, known)) conditions.push(`${left}.${q(column.name)} = ${right}.${q(column.references!.column)}`);
    }
    for (const column of known.columns) {
      if (points(column, joined)) conditions.push(`${left}.${q(column.references!.column)} = ${right}.${q(column.name)}`);
    }
  }
  return [...new Set(conditions)].map((condition) => ({
    label: condition,
    type: "columnForeignKey",
    detail: t("ui.sqlCompletion.joinCondition"),
    boost: BOOST.joinCondition,
  }));
}

/** After `name.`: the columns of a relation or CTE of the query, of a table by name, or what a schema holds. */
export function qualifiedOptions(
  schema: CompletionSchema | null,
  ctx: QueryContext,
  dialect: SqlDialect,
  qualifier: string,
  qualifierSchema: string | null = null,
): Completion[] {
  if (qualifierSchema) {
    // `schema.table.`: that schema's table, whatever else the query reads
    const table = schema && resolve(schema, { schema: qualifierSchema, name: qualifier });
    return table ? table.columns.map((column) => columnOption(column, dialect, null)) : [];
  }
  const inQuery =
    ctx.relations.find((r) => r.alias && same(r.alias, qualifier)) ??
    ctx.relations.find((r) => !r.alias && same(r.name, qualifier) && (ctx.ctes.some((c) => same(c.name, r.name)) || (schema && resolve(schema, r))));
  if (inQuery) return columnsOf(schema, ctx, inQuery, dialect, null);
  const cte = ctx.ctes.find((c) => same(c.name, qualifier));
  if (cte) return cteColumns(cte.name, cte.columns, dialect, null);
  if (!schema) return [];
  const holder = indexOf(schema).schemas.find((s) => same(s, qualifier));
  if (holder) {
    const relations: Completion[] = schema.relations
      .filter((r) => r.schema === holder)
      .map((r) => ({ label: r.name, type: r.type, detail: label(r.type), apply: applyName(r.name, dialect), boost: BOOST.wanted }));
    const functions: Completion[] = schema.functions
      .filter((f) => f.schema === holder)
      .map((f) => ({ label: f.name, type: f.kind, detail: signatureOf(f), apply: applyName(f.name, dialect), boost: BOOST.userFunction }));
    return [...relations, ...functions];
  }
  const table = resolve(schema, { schema: null, name: qualifier });
  return table ? table.columns.map((column) => columnOption(column, dialect, null)) : [];
}

const signatureOf = (f: CompletionSchema["functions"][number]) => `(${f.argumentTypes})${f.returnType ? ` → ${f.returnType}` : ""}`;

const functionCache = new WeakMap<object, Map<string, Completion[]>>();
const NO_SCHEMA = {};

/** The catalog's functions with their description, then the user's, then names only the server knows. */
export function functionOptions(schema: CompletionSchema | null, dialect: SqlDialect, features: SqlFeature[]): Completion[] {
  const key = `${dialect.engine}|${features.join(",")}`;
  const byKey = functionCache.get(schema ?? NO_SCHEMA) ?? new Map<string, Completion[]>();
  functionCache.set(schema ?? NO_SCHEMA, byKey);
  const cached = byKey.get(key);
  if (cached) return cached;

  const seen = new Set<string>();
  const options: Completion[] = available(dialect.functions, features).map((f) => {
    seen.add(f.name.toLowerCase());
    const description = tr(`sqlFunctions.${dialect.engine}.${functionKey(f.name)}`);
    return { label: f.name, type: "builtinFunction", detail: label("builtinFunction"), info: `${f.signature}\n\n${description}`, boost: BOOST.catalogFunction };
  });
  if (schema) {
    const users = new Set<string>();
    for (const f of schema.functions) {
      const name = f.schema === schema.currentSchema ? f.name : `${f.schema}.${f.name}`;
      if (users.has(name)) continue; // overloads come once
      users.add(name);
      const apply = f.schema === schema.currentSchema ? applyName(f.name, dialect) : `${dialect.quoteIdent(f.schema)}.${dialect.quoteIdent(f.name)}`;
      options.push({ label: name, type: f.kind, detail: signatureOf(f), apply: apply === name ? undefined : apply, boost: BOOST.userFunction });
    }
    for (const name of schema.builtinFunctions) {
      const lower = name.toLowerCase();
      if (seen.has(lower) || users.has(name)) continue;
      seen.add(lower);
      options.push({ label: name, type: "builtinFunction", detail: label("builtinFunction"), boost: BOOST.serverFunction });
    }
  }
  byKey.set(key, options);
  return options;
}

const keywordCache = new Map<string, Completion[]>();

/** The dialect's keywords, data types and the phrases the server has. */
export function keywordOptions(dialect: SqlDialect, features: SqlFeature[]): Completion[] {
  const key = `${dialect.engine}|${features.join(",")}`;
  const cached = keywordCache.get(key);
  if (cached) return cached;
  const options = new Map<string, Completion>();
  for (const word of dialect.keywords) options.set(word, { label: word, type: "keyword", detail: label("keyword"), boost: BOOST.keyword });
  for (const word of dialect.types) options.set(word, { label: word, type: "dataType", detail: label("dataType"), boost: BOOST.dataType });
  for (const phrase of available(dialect.phrases, features)) {
    options.set(phrase.label, { label: phrase.label, type: "keyword", detail: label("keyword"), boost: BOOST.phrase });
  }
  const list = [...options.values()];
  keywordCache.set(key, list);
  return list;
}

/** The snippets each position starts: statements at the start, `case` in an expression, `join` after a relation. */
const SNIPPETS_AT: Partial<Record<Position["kind"], (id: string) => boolean>> = {
  statementStart: (id) => id !== "case" && id !== "join",
  column: (id) => id === "case",
  joinCondition: (id) => id === "case",
  keyword: (id) => id === "join",
};

/** The dialect's statement templates that fit the position and the server. */
export function snippetOptions(dialect: SqlDialect, features: SqlFeature[], position: Position): Completion[] {
  const fits = SNIPPETS_AT[position.kind];
  if (!fits) return [];
  return available(dialect.snippets, features)
    .filter((s) => fits(s.id))
    .map((s) =>
      snippetCompletion(s.template, {
        label: s.label,
        type: "snippet",
        detail: tr(`sqlSnippets.${s.id}`),
        info: s.template.replace(/\$\{(\w+)\}/g, "$1"),
        boost: BOOST.snippet,
      }),
    );
}

/** Kinds that name something, so they still fit inside a quoted name. */
const NAMES = new Set(["schema", "table", "view", "materializedView", "column", "columnPrimaryKey", "columnForeignKey", "function", "procedure", "local"]);

/**
 * The options that fit inside a quote already opened: single names only (`schema.table` and
 * join conditions would end up inside one pair of quotes), written without their own quotes.
 */
export function insideQuotes(options: Completion[], close: string): Completion[] {
  return options
    .filter((o) => NAMES.has(String(o.type)) && !o.label.includes("."))
    .map((o) => ({ ...o, apply: o.label.split(close).join(close + close) }));
}

/** Everything worth offering at the cursor, ranked by `boost`. */
export function optionsAt(schema: CompletionSchema | null, ctx: QueryContext, dialect: SqlDialect, features: SqlFeature[]): Completion[] {
  const position = ctx.position;
  switch (position.kind) {
    case "quiet":
      return [];
    case "statementStart":
    case "keyword":
      return [...snippetOptions(dialect, features, position), ...keywordOptions(dialect, features)];
    case "table":
      return [...relationOptions(schema, ctx, dialect), ...keywordOptions(dialect, features)];
    case "qualified":
      return qualifiedOptions(schema, ctx, dialect, position.qualifier, position.schema);
    case "insertColumns": {
      const present = new Set(position.present.map((name) => name.toLowerCase()));
      const columns = columnsOf(schema, ctx, position.table, dialect, null);
      return columns.filter((option) => !present.has(option.label.toLowerCase()));
    }
    case "joinCondition":
      return [
        ...joinConditionOptions(schema, ctx, dialect),
        ...columnOptions(schema, ctx, dialect),
        ...functionOptions(schema, dialect, features),
        ...snippetOptions(dialect, features, position),
        ...keywordOptions(dialect, features),
      ];
    case "column":
      return [
        ...columnOptions(schema, ctx, dialect),
        ...functionOptions(schema, dialect, features),
        ...snippetOptions(dialect, features, position),
        ...keywordOptions(dialect, features),
      ];
  }
}
