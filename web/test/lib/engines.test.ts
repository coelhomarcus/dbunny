import { test } from "node:test";
import assert from "node:assert/strict";
import { addressOf, dialectData, ENGINE_CARDS, engineLabel, locationOf, sqlDialectOf } from "../../src/lib/engines.ts";

test("mysql is available and mariadb is coming soon", () => {
  const card = (id: string) => ENGINE_CARDS.find((c) => c.id === id);
  assert.deepEqual([card("mysql")?.label, card("mysql")?.kind, card("mysql")?.available], ["MySQL", "Server connection", true]);
  assert.deepEqual([card("mariadb")?.label, card("mariadb")?.available], ["MariaDB", false]);
  assert.equal(card("postgres")?.available, true);
  assert.equal(engineLabel("mysql"), "MySQL");
});

test("mysql is located like a server", () => {
  const info = { engine: "mysql" as const, host: "db.local", port: 3306, database: "shop" };
  assert.deepEqual(locationOf(info), { where: "db.local:3306", database: "shop" });
  assert.equal(addressOf(info), "db.local:3306/shop");
  assert.equal(addressOf({ ...info, database: "" }), "db.local:3306");
  assert.equal(addressOf({ engine: "sqlite", host: "/x.db", port: 0, database: "" }), "/x.db");
});

test("the sql dialect follows the engine", () => {
  assert.equal(sqlDialectOf("sqlite"), "sqlite");
  assert.equal(sqlDialectOf("postgres"), "postgres");
  assert.equal(sqlDialectOf("mysql"), "mysql");
  assert.equal(sqlDialectOf(undefined), "postgres");
});

test("postgres and sqlite dialect data stay as the screens had it", () => {
  const pg = dialectData("postgres");
  assert.deepEqual(pg.suggestedTypes, ["text", "varchar(255)", "integer", "bigint", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"]);
  assert.deepEqual(pg.newTableTypes, ["bigserial", "bigint", "integer", "text", "varchar(255)", "numeric(12,2)", "boolean", "date", "timestamptz", "jsonb", "uuid"]);
  assert.deepEqual([pg.defaultTextType, pg.defaultIndexMethod, pg.indexMethods], ["text", "btree", ["btree", "hash", "gin", "gist", "brin"]]);
  assert.deepEqual(pg.addColumnGenerations, ["none", "identityByDefault", "identityAlways", "generated"]);
  assert.deepEqual(pg.newTableGenerations, ["none", "identityByDefault", "identityAlways", "generated"]);
  assert.deepEqual(pg.createTableFlags, ["unlogged", "partition"]);
  assert.deepEqual(pg.starterColumns.map((c) => [c.name, c.dataType, c.default ?? null]), [["id", "bigserial", null], ["created_at", "timestamptz", "now()"]]);

  const lite = dialectData("sqlite");
  assert.deepEqual(lite.suggestedTypes, ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"]);
  assert.deepEqual(lite.newTableTypes, ["INTEGER", "TEXT", "REAL", "NUMERIC", "BLOB"]);
  assert.deepEqual([lite.defaultTextType, lite.defaultIndexMethod, lite.indexMethods, lite.addColumnGenerations], ["TEXT", "btree", [], []]);
  assert.deepEqual(lite.newTableGenerations, ["none", "autoincrement", "generated"]);
  assert.deepEqual(lite.createTableFlags, ["withoutRowid", "strict"]);
  assert.deepEqual(lite.starterColumns.map((c) => [c.name, c.dataType, c.default ?? null]), [["id", "INTEGER", null], ["created_at", "TEXT", "CURRENT_TIMESTAMP"]]);
});

test("mysql dialect data", () => {
  const my = dialectData("mysql");
  assert.deepEqual(my.suggestedTypes, ["int", "bigint", "varchar(255)", "text", "decimal(12,2)", "tinyint(1)", "date", "datetime", "timestamp", "json", "blob"]);
  assert.deepEqual(my.newTableTypes, my.suggestedTypes);
  assert.deepEqual([my.defaultTextType, my.defaultIndexMethod, my.indexMethods], ["varchar(255)", "BTREE", ["BTREE", "HASH", "FULLTEXT", "SPATIAL"]]);
  assert.deepEqual(my.addColumnGenerations, ["none", "autoincrement", "generated"]);
  assert.deepEqual(my.newTableGenerations, ["none", "autoincrement", "generated"]);
  assert.deepEqual(my.createTableFlags, []);
  const id = my.starterColumns[0];
  assert.deepEqual([id.name, id.dataType, id.primaryKey, id.generation], ["id", "bigint", true, "autoincrement"]);
});
