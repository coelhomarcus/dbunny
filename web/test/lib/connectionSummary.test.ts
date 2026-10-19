import { test } from "node:test";
import assert from "node:assert/strict";
import type { SavedConnection } from "@/types";
import { fileNameOf, mask, summaryOf } from "../../src/lib/connectionSummary.ts";

const saved: SavedConnection = {
  id: "c1",
  name: "Prod",
  engine: "postgres",
  path: "",
  host: "db.example.com",
  port: 5433,
  database: "app",
  user: "me",
  password: "pw",
  ssh: null,
  tls: null,
  ssl: false,
  readOnly: false,
  color: "#ef4444",
  icon: "",
  createdAt: "",
  updatedAt: "",
};

test("the address shows host, port and database", () => {
  assert.equal(summaryOf(saved).address, "db.example.com:5433/app");
});

test("a socket shows its folder", () => {
  assert.equal(summaryOf({ ...saved, host: "/tmp" }).address, "/tmp (socket) / app");
});

test("ssh shows who it logs in as", () => {
  const ssh = { host: "bastion", port: 2222, user: "ubuntu", authMethod: "agent", password: "", keyPath: "", passphrase: "" } as const;
  assert.equal(summaryOf({ ...saved, ssh }).sshVia, "ubuntu@bastion:2222");
  assert.equal(summaryOf({ ...saved, ssh: { ...ssh, port: 22 } }).sshVia, "ubuntu@bastion");
  assert.equal(summaryOf(saved).sshVia, null);
});

test("the password never appears", () => {
  assert.ok(!JSON.stringify(summaryOf({ ...saved, password: "s3cr3t-value" })).includes("s3cr3t-value"));
});

test("mask hides every character", () => {
  assert.equal(mask("db:5432"), "•••••••");
});

test("the summary of a sqlite connection is its file", () => {
  const sqlite: SavedConnection = {
    ...saved,
    engine: "sqlite",
    host: "",
    port: 0,
    database: "",
    user: "",
    password: "",
    path: "/Users/me/notes.sqlite",
    readOnly: true,
  };
  const summary = summaryOf(sqlite);
  assert.equal(summary.engineLabel, "SQLite");
  assert.equal(summary.address, "/Users/me/notes.sqlite");
  assert.equal(summary.user, "");
  assert.equal(summary.ssl, false);
  assert.equal(summary.sshVia, null);
  assert.equal(summary.readOnly, true);
  const { address, ...rest } = summary;
  assert.ok(address);
  assert.ok(!JSON.stringify(rest).includes("notes.sqlite"));
});

test("a file name is the last segment of its path", () => {
  assert.equal(fileNameOf("/Users/me/notes.sqlite"), "notes.sqlite");
  assert.equal(fileNameOf("C:\\data\\shop.db"), "shop.db");
  assert.equal(fileNameOf("plain.db"), "plain.db");
});

test("a mysql connection is a server, with the database when it has one", () => {
  const mysql = { ...saved, engine: "mysql" as const, host: "db.local", port: 3307, database: "shop", ssl: true };
  assert.equal(summaryOf(mysql).engineLabel, "MySQL");
  assert.equal(summaryOf(mysql).address, "db.local:3307/shop");
  assert.equal(summaryOf({ ...mysql, database: "" }).address, "db.local:3307");
  assert.equal(summaryOf({ ...mysql, host: "/var/run/mysqld/mysqld.sock", database: "" }).address, "/var/run/mysqld/mysqld.sock (socket)");
  assert.equal(summaryOf({ ...mysql, host: "/var/run/mysqld/mysqld.sock" }).address, "/var/run/mysqld/mysqld.sock (socket) / shop");
  assert.equal(summaryOf(mysql).ssl, true);
});

test("a mariadb connection is a server too", () => {
  const mariadb = { ...saved, engine: "mariadb" as const, host: "db.local", port: 3306, database: "shop", ssl: false };
  assert.equal(summaryOf(mariadb).engineLabel, "MariaDB");
  assert.equal(summaryOf(mariadb).address, "db.local:3306/shop");
  assert.equal(summaryOf({ ...mariadb, database: "" }).address, "db.local:3306");
});
