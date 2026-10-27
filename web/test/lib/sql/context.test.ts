import test from "node:test";
import assert from "node:assert/strict";
import { analyze } from "../../../src/lib/sql/completion/context.ts";
import { dialectOf } from "../../../src/lib/sql/dialects/index.ts";
import type { Engine, SqlFeature } from "../../../src/types/index.ts";

/** Analyzes `sql` with the cursor where `|` is. */
function at(sql: string, engine: Engine = "postgres", features: SqlFeature[] = []) {
  const pos = sql.indexOf("|");
  return analyze(sql.slice(0, pos) + sql.slice(pos + 1), pos, dialectOf(engine), features);
}
const kind = (sql: string, engine: Engine = "postgres", features: SqlFeature[] = []) => at(sql, engine, features).position.kind;

test("tables after from and join", () => {
  for (const engine of ["postgres", "mysql"] as const) {
    assert.deepEqual(at("SELECT * FROM |", engine).position, { kind: "table", afterJoin: false });
    assert.deepEqual(at("SELECT * FROM pedidos p JOIN |", engine).position, { kind: "table", afterJoin: true });
    assert.deepEqual(at("SELECT * FROM pedidos p LEFT JOIN cli|", engine).position, { kind: "table", afterJoin: true });
    assert.deepEqual(at("SELECT * FROM a, |", engine).position, { kind: "table", afterJoin: false });
    assert.equal(kind("UPDATE |", engine), "table");
    assert.equal(kind("INSERT INTO |", engine), "table");
    assert.equal(kind("DELETE FROM |", engine), "table");
    assert.equal(kind("TRUNCATE TABLE |", engine), "table");
  }
  assert.equal(kind("SELECT * FROM a |"), "keyword");
  assert.equal(kind("SELECT * FROM a AS |"), "quiet");
});

test("join condition", () => {
  const { position } = at("SELECT * FROM pedidos p JOIN clientes c ON |");
  assert.deepEqual(position, { kind: "joinCondition", target: { schema: null, name: "clientes", alias: "c" } });
  assert.equal(kind("SELECT * FROM pedidos p JOIN clientes c ON c.id = |"), "column");
  assert.equal(kind("SELECT * FROM pedidos p JOIN clientes c ON c.id = p.cliente_id AND |"), "column");
});

test("qualified by alias, table or cte", () => {
  assert.deepEqual(at("SELECT p.| FROM pedidos p").position, { kind: "qualified", qualifier: "p" });
  assert.deepEqual(at("SELECT p.na| FROM pedidos p").position, { kind: "qualified", qualifier: "p" });
  assert.deepEqual(at('SELECT "Pedidos".| FROM "Pedidos"').position, { kind: "qualified", qualifier: "Pedidos" });
  assert.deepEqual(at("SELECT * FROM vendas.|").position, { kind: "qualified", qualifier: "vendas" });
  assert.deepEqual(at("SELECT `p`.| FROM pedidos p", "mysql").position, { kind: "qualified", qualifier: "p" });
});

test("columns in clauses", () => {
  assert.equal(kind("SELECT |"), "column");
  assert.equal(kind("SELECT a, |"), "column");
  assert.equal(kind("SELECT count(|"), "column");
  assert.equal(kind("SELECT * FROM a WHERE |"), "column");
  assert.equal(kind("SELECT * FROM a WHERE a = 1 AND |"), "column");
  assert.equal(kind("SELECT * FROM a ORDER BY a, |"), "column");
  assert.equal(kind("SELECT * FROM a GROUP BY |"), "column");
  assert.equal(kind("UPDATE a SET |"), "column");
  assert.equal(kind("SELECT * FROM a WHERE id IN (SELECT x FROM |"), "table");
  assert.equal(kind("SELECT a |"), "keyword");
  assert.equal(kind("DELETE FROM a RETURNING |", "postgres", ["returning"]), "column");
  assert.equal(kind("DELETE FROM a RETURNING |", "mysql", []), "keyword");
});

test("insert column list", () => {
  const { position } = at("INSERT INTO pedidos (id, |");
  assert.deepEqual(position, { kind: "insertColumns", table: { schema: null, name: "pedidos", alias: null }, present: ["id"] });
  const quoted = at("INSERT INTO `loja`.`pedidos` (`id`, total, |", "mysql").position;
  assert.deepEqual(quoted, { kind: "insertColumns", table: { schema: "loja", name: "pedidos", alias: null }, present: ["id", "total"] });
  assert.equal(kind("INSERT INTO pedidos (id) VALUES (|"), "column");
});

test("statement start", () => {
  assert.equal(kind("|"), "statementStart");
  assert.equal(kind("SEL|"), "statementStart");
  assert.equal(kind("SELECT 1; |"), "statementStart");
  assert.equal(kind("SELECT 1;\n|\nSELECT 2"), "statementStart");
  assert.equal(kind("WITH t AS (|"), "statementStart");
  assert.equal(kind("WITH t AS (SELECT 1) |"), "keyword");
  assert.equal(at("SEL|").word.text, "SEL");
});

test("quiet in strings and comments", () => {
  assert.equal(kind("SELECT 'abc|'"), "quiet");
  assert.equal(kind("SELECT 'it''s |"), "quiet");
  assert.equal(kind("SELECT 1 -- note |"), "quiet");
  assert.equal(kind("SELECT /* x | */ 1"), "quiet");
  assert.equal(kind("SELECT /* x */ |"), "column");
  assert.equal(kind("SELECT 1 # note |", "mysql"), "quiet");
  assert.equal(kind("SELECT 1 # |", "postgres"), "column");
  assert.equal(kind("SELECT \"abc |\"", "mysql"), "quiet");
  assert.equal(kind("SELECT 'a\\' |'", "mysql"), "quiet");
  assert.equal(kind("CREATE FUNCTION f() RETURNS int AS $$ SELECT | $$ LANGUAGE sql"), "quiet");
  assert.equal(kind("SELECT $body$ x | $body$"), "quiet");
  assert.equal(kind("SELECT E'a\\' |'"), "quiet");
});

test("backticks and aliases", () => {
  const { relations } = at("SELECT | FROM `pedidos` AS p", "mysql");
  assert.deepEqual(relations, [{ schema: null, name: "pedidos", alias: "p" }]);
  const many = at("SELECT | FROM a x, b AS y JOIN c ON c.id = x.c WHERE 1").relations;
  assert.deepEqual(many, [
    { schema: null, name: "a", alias: "x" },
    { schema: null, name: "b", alias: "y" },
    { schema: null, name: "c", alias: null },
  ]);
  assert.deepEqual(at("UPDATE pedidos SET |").relations, [{ schema: null, name: "pedidos", alias: null }]);
  // A double-quoted text is a string in MySQL, not a name
  assert.deepEqual(at('SELECT | FROM "pedidos"', "mysql").relations, []);
});

test("ctes", () => {
  const { ctes, relations } = at("WITH totais(cliente, soma) AS (SELECT c, sum(v) FROM vendas GROUP BY c) SELECT | FROM totais");
  assert.deepEqual(ctes, [{ name: "totais", columns: ["cliente", "soma"] }]);
  // vendas is inside the CTE's body, out of the cursor's reach
  assert.deepEqual(relations, [{ schema: null, name: "totais", alias: null }]);
  const two = at("WITH RECURSIVE a AS (SELECT 1), b AS MATERIALIZED (SELECT 2) SELECT | FROM a, b").ctes;
  assert.deepEqual(two.map((c) => c.name), ["a", "b"]);
});

test("a correlated subquery sees the outer relations", () => {
  const { relations } = at("SELECT * FROM pedidos p WHERE EXISTS (SELECT 1 FROM itens i WHERE i.pedido_id = |)");
  assert.deepEqual(relations.map((r) => r.name).sort(), ["itens", "pedidos"]);
});

test("same name in two schemas", () => {
  const { relations } = at("SELECT | FROM vendas.pedidos p JOIN compras.pedidos q ON true");
  assert.deepEqual(relations, [
    { schema: "vendas", name: "pedidos", alias: "p" },
    { schema: "compras", name: "pedidos", alias: "q" },
  ]);
});

test("cursor between statements", () => {
  const { relations } = at("SELECT 1 FROM a; SELECT | FROM b; SELECT 2 FROM c");
  assert.deepEqual(relations, [{ schema: null, name: "b", alias: null }]);
  assert.deepEqual(at("SELECT 1 FROM a; SELECT x FROM b;|").relations, []);
});

test("the word under the cursor", () => {
  assert.deepEqual(at("SELECT * FROM ped|").word, { from: 14, text: "ped" });
  assert.deepEqual(at('SELECT * FROM "Ped|').word, { from: 14, text: '"Ped' });
  assert.deepEqual(at("SELECT * FROM |").word, { from: 14, text: "" });
});
