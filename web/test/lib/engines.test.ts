import { test } from "node:test";
import assert from "node:assert/strict";
import { addressOf, ENGINE_CARDS, engineLabel, locationOf, sqlDialectOf } from "../../src/lib/engines.ts";

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
