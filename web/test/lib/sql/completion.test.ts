import test from "node:test";
import assert from "node:assert/strict";
import type { Completion } from "@codemirror/autocomplete";
import { analyze } from "../../../src/lib/sql/completion/context.ts";
import { ICON_LABEL, ITEM_KINDS, iconSvg } from "../../../src/lib/sql/completion/icons.ts";
import { insideQuotes, optionsAt, snippetOptions } from "../../../src/lib/sql/completion/sources.ts";
import { dialectOf } from "../../../src/lib/sql/dialects/index.ts";
import { en } from "../../../src/i18n/locales/en.ts";
import { ptBR } from "../../../src/i18n/locales/pt-BR.ts";
import type { CompletionColumn, CompletionRelation, CompletionSchema, Engine, SqlFeature } from "../../../src/types/index.ts";

const col = (name: string, dataType: string, extra: Partial<CompletionColumn> = {}): CompletionColumn => ({
  name, dataType, primaryKey: false, nullable: true, references: null, ...extra,
});

const SCHEMA: CompletionSchema = {
  currentSchema: "public",
  relations: [
    { schema: "public", name: "parents", type: "table", columns: [col("id", "integer", { primaryKey: true, nullable: false }), col("name", "text")] },
    {
      schema: "public", name: "children", type: "table",
      columns: [
        col("id", "integer", { primaryKey: true, nullable: false }),
        col("parent_id", "integer", { nullable: false, references: { schema: "public", table: "parents", column: "id" } }),
        col("user", "text"),
      ],
    },
    { schema: "public", name: "unrelated", type: "table", columns: [col("x", "int")] },
    { schema: "public", name: "parent_names", type: "view", columns: [col("name", "text")] },
    { schema: "audit", name: "events", type: "table", columns: [col("at", "timestamp")] },
  ],
  functions: [
    { schema: "public", name: "twice", returnType: "integer", argumentTypes: "n integer", kind: "function" },
    { schema: "public", name: "touch", returnType: "", argumentTypes: "", kind: "procedure" },
  ],
  builtinFunctions: ["date_trunc", "length", "pg_backend_pid"],
};

/** The options at `|` in `sql`. */
function at(sql: string, engine: Engine = "postgres", features: SqlFeature[] = [], schema: CompletionSchema | null = SCHEMA): Completion[] {
  const pos = sql.indexOf("|");
  const doc = sql.slice(0, pos) + sql.slice(pos + 1);
  const dialect = dialectOf(engine);
  return optionsAt(schema, analyze(doc, pos, dialect, features), dialect, features);
}
const labels = (options: Completion[]) => options.map((o) => o.label);
const ranked = (options: Completion[]) => [...options].sort((a, b) => (b.boost ?? 0) - (a.boost ?? 0));
const find = (options: Completion[], label: string) => options.find((o) => o.label === label);

test("columns in scope carry their keys", () => {
  const options = at("SELECT | FROM children");
  const parent = find(options, "parent_id")!;
  assert.equal(parent.type, "columnForeignKey");
  assert.match(String(parent.detail), /FK → parents/);
  assert.equal(find(options, "id")!.type, "columnPrimaryKey");
  assert.equal(find(options, "user")!.type, "column");
  // Columns of tables the query doesn't read stay out
  assert.equal(find(options, "x"), undefined);
  // Columns come before functions and keywords
  assert.ok(ranked(options).findIndex((o) => o.label === "parent_id") < ranked(options).findIndex((o) => o.label === "length"));
});

test("join conditions come from foreign keys", () => {
  const options = ranked(at("SELECT * FROM children c JOIN parents p ON |"));
  assert.equal(options[0].label, "p.id = c.parent_id");
  const reverse = ranked(at("SELECT * FROM parents p JOIN children c ON |"));
  assert.equal(reverse[0].label, "c.parent_id = p.id");
});

test("tables after join rank related ones first", () => {
  const options = ranked(at("SELECT * FROM children c JOIN |"));
  const relations = options.filter((o) => ["table", "view", "materializedView"].includes(String(o.type)));
  assert.equal(relations[0].label, "parents");
  assert.ok(labels(options).includes("audit.events"));
  assert.equal(find(options, "parent_names")!.type, "view");
});

test("functions merge the catalog, the user's and the server's", () => {
  const options = at("SELECT |");
  const catalog = find(options, "date_trunc")!;
  const user = find(options, "twice")!;
  const server = find(options, "pg_backend_pid")!;
  assert.equal(catalog.type, "builtinFunction");
  assert.ok(catalog.info, "a catalog function has a description");
  assert.equal(user.type, "function");
  assert.equal(server.type, "builtinFunction");
  assert.equal(server.info, undefined);
  assert.ok((catalog.boost ?? 0) > (user.boost ?? 0) && (user.boost ?? 0) > (server.boost ?? 0));
  assert.equal(options.filter((o) => o.label === "date_trunc").length, 1);
  assert.equal(options.filter((o) => o.label === "length").length, 1);
  assert.equal(find(options, "touch")!.type, "procedure");
});

test("no suggestion leaks from another dialect", () => {
  const everything = (engine: Engine, schema = SCHEMA) => [
    ...at("SELECT |", engine, ["windowFunctions", "recursiveCte"], schema),
    ...at("SELECT * FROM a |", engine, [], schema),
    ...at("|", engine, [], schema),
  ].map((o) => String(o.label).toLowerCase());
  // A MySQL or MariaDB server never sends built-in names; the PostgreSQL ones above would be its own
  const mysqlSchema = { ...SCHEMA, builtinFunctions: [] };
  for (const engine of ["mysql", "mariadb"] as const) {
    const words = everything(engine, mysqlSchema);
    for (const leak of ["on conflict", "nulls first", "ilike", "date_trunc", "insert or replace"]) assert.ok(!words.includes(leak), `${leak} in ${engine}`);
  }
  for (const leak of ["date_format", "on duplicate key update", "group_concat"]) assert.ok(!everything("postgres").includes(leak), `${leak} in postgres`);
  const sqliteSchema = { ...SCHEMA, functions: [], builtinFunctions: [] };
  assert.ok(!at("SELECT |", "sqlite", [], sqliteSchema).some((o) => o.type === "procedure"));
});

test("requires gates items", () => {
  assert.equal(find(at("SELECT |", "mysql", [], null), "JSON_TABLE"), undefined);
  assert.ok(find(at("SELECT |", "mysql", ["jsonTable"], null), "JSON_TABLE"));
  assert.equal(find(at("SELECT * FROM a ORDER BY b |", "postgres", []), "NULLS FIRST"), undefined);
  assert.ok(find(at("SELECT * FROM a ORDER BY b |", "postgres", ["nullsOrdering"]), "NULLS FIRST"));
});

test("applies quoted names per dialect", () => {
  assert.equal(find(at("SELECT | FROM children", "mysql"), "user")!.apply, "`user`");
  assert.equal(find(at("SELECT | FROM children", "postgres"), "user")!.apply, '"user"');
  assert.equal(find(at("SELECT | FROM children", "postgres"), "parent_id")!.apply, undefined);
  assert.equal(find(at("SELECT | FROM children", "postgres"), "id")!.apply, undefined);
  assert.equal(find(at("SELECT * FROM |", "postgres"), "audit.events")!.apply, undefined);
});

test("qualified names list what the qualifier holds", () => {
  assert.deepEqual(labels(at("SELECT c.| FROM children c")).sort(), ["id", "parent_id", "user"]);
  assert.deepEqual(labels(at("SELECT * FROM audit.|")), ["events"]);
});

test("insert lists the columns still missing", () => {
  assert.deepEqual(labels(at("INSERT INTO children (id, |")).sort(), ["parent_id", "user"]);
});

test("strings, comments and fresh names stay quiet", () => {
  assert.deepEqual(at("SELECT '|'"), []);
  assert.deepEqual(at("SELECT 1 -- |"), []);
});

test("every kind has an icon and a label", () => {
  assert.equal(ITEM_KINDS.length, 14);
  for (const kind of ITEM_KINDS) {
    assert.match(iconSvg(kind), /^<svg[\s\S]*aria-hidden="true"/);
    const [area, screen, role] = ICON_LABEL[kind].split(".");
    assert.ok((en as unknown as Record<string, Record<string, Record<string, string>>>)[area][screen][role], kind);
  }
});

test("works before the schema loads", () => {
  const options = at("SELECT |", "postgres", [], null);
  assert.ok(find(options, "date_trunc"));
  assert.ok(at("SELECT * FROM a |", "postgres", [], null).some((o) => o.label === "LEFT JOIN"));
  assert.ok(at("SELECT * FROM a |", "postgres", [], null).some((o) => o.label === "WHERE"));
});

test("large schema builds quickly", () => {
  const relations: CompletionRelation[] = [];
  for (let r = 0; r < 2000; r++) {
    relations.push({ schema: "public", name: `t${r}`, type: "table", columns: Array.from({ length: 20 }, (_, c) => col(`c${c}`, "int")) });
  }
  const big: CompletionSchema = {
    currentSchema: "public", relations, functions: [],
    builtinFunctions: Array.from({ length: 3000 }, (_, i) => `f${i}`),
  };
  at("SELECT | FROM t1", "postgres", [], big); // warm-up builds the indexes once per schema
  const started = performance.now();
  at("SELECT | FROM t1 JOIN t2 ON true", "postgres", [], big);
  at("SELECT * FROM |", "postgres", [], big);
  assert.ok(performance.now() - started < 50, `took ${performance.now() - started} ms`);
});

const snippet = (engine: Engine, id: string, features: SqlFeature[] = []) => dialectOf(engine).snippets.find((s) => s.id === id && (!s.requires || features.includes(s.requires)));

test("snippets follow each dialect", () => {
  assert.match(snippet("postgres", "upsert", ["onConflict"])!.template, /ON CONFLICT/);
  assert.match(snippet("sqlite", "upsert", ["onConflict"])!.template, /ON CONFLICT/);
  for (const engine of ["mysql", "mariadb"] as const) {
    assert.match(snippet(engine, "upsert", ["onDuplicateKeyUpdate"])!.template, /ON DUPLICATE KEY UPDATE/);
    assert.doesNotMatch(snippet(engine, "upsert", ["onDuplicateKeyUpdate"])!.template, /ON CONFLICT/);
    assert.match(snippet(engine, "createTable")!.template, /AUTO_INCREMENT/);
  }
  assert.match(snippet("postgres", "createTable")!.template, /GENERATED ALWAYS AS IDENTITY/);
  assert.match(snippet("sqlite", "createTable")!.template, /INTEGER PRIMARY KEY/);
  const statement = (engine: Engine, features: SqlFeature[]) =>
    snippetOptions(dialectOf(engine), features, { kind: "statementStart" }).map((o) => o.label);
  assert.ok(!statement("postgres", []).includes("merge"));
  assert.ok(statement("postgres", ["merge"]).includes("merge"));
  assert.ok(!statement("mysql", ["merge"]).includes("merge"));
});

test("every snippet has an example and words in both languages", () => {
  for (const engine of ["postgres", "mysql", "mariadb", "sqlite"] as const) {
    const { snippets } = dialectOf(engine);
    assert.ok(snippets.length >= 9, engine);
    for (const s of snippets) {
      assert.ok(s.example.trim() && s.template.includes("${"), `${engine} ${s.id}`);
      assert.ok((en.sqlSnippets as Record<string, string>)[s.id], `en sqlSnippets.${s.id}`);
      assert.ok((ptBR.sqlSnippets as Record<string, string>)[s.id], `pt-BR sqlSnippets.${s.id}`);
    }
  }
});

test("snippets open a statement and rank below names", () => {
  const start = at("sel|");
  const select = find(start, "select")!;
  assert.equal(select.type, "snippet");
  assert.equal(start.filter((o) => o.type === "snippet").length, 6);
  // upsert and merge need the server's onConflict and merge
  assert.equal(at("sel|", "postgres", ["onConflict", "merge"]).filter((o) => o.type === "snippet").length, 8);
  const withColumn = { ...SCHEMA, relations: [{ schema: "public", name: "t", type: "table" as const, columns: [col("cased", "bool")] }] };
  const options = ranked(at("SELECT cas| FROM t", "postgres", [], withColumn));
  const column = options.findIndex((o) => o.label === "cased");
  const caseSnippet = options.findIndex((o) => o.label === "case" && o.type === "snippet");
  assert.ok(caseSnippet >= 0 && column < caseSnippet);
  // A snippet ranks above the keyword it starts with
  assert.ok(caseSnippet < options.findIndex((o) => o.label === "CASE"));
});

test("join conditions write each relation the way the query does", () => {
  const upper = { ...SCHEMA, relations: SCHEMA.relations.map((r) => ({ ...r })) };
  // PostgreSQL folds unquoted PARENTS and Children to lower case: they must not come back quoted
  const options = ranked(at("SELECT * FROM Children JOIN PARENTS ON |", "postgres", [], upper));
  assert.equal(options[0].label, "PARENTS.id = Children.parent_id");
  assert.equal(find(at("SELECT * FROM PARENTS WHERE |", "postgres", [], upper), "PARENTS")!.apply, undefined);
});

test("a schema-qualified table lists that schema's columns", () => {
  const twin: CompletionSchema = {
    ...SCHEMA,
    relations: [
      { schema: "public", name: "orders", type: "table", columns: [col("pub_only", "int")] },
      { schema: "sales", name: "orders", type: "table", columns: [col("sales_only", "int")] },
    ],
  };
  assert.deepEqual(labels(at("SELECT sales.orders.| FROM public.orders JOIN sales.orders ON true", "postgres", [], twin)), ["sales_only"]);
  assert.deepEqual(labels(at("SELECT sales.orders.|", "postgres", [], twin)), ["sales_only"]);
  assert.deepEqual(labels(at("SELECT public.orders.| FROM sales.orders", "postgres", [], twin)), ["pub_only"]);
});

test("inside an opened quote only single names are offered, without new quotes", () => {
  const options: Completion[] = [
    { label: "Other Table", type: "table" },
    { label: "sales.Other Table", type: "table" },
    { label: "p.id = c.parent_id", type: "columnForeignKey" },
    { label: 'say "hi"', type: "column" },
    { label: "SELECT", type: "keyword" },
  ];
  assert.deepEqual(insideQuotes(options, '"').map((o) => [o.label, o.apply]), [
    ["Other Table", "Other Table"],
    ['say "hi"', 'say ""hi""'],
  ]);
});
