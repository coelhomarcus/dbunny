import { test } from "node:test";
import assert from "node:assert/strict";
import type { SavedConnection } from "@/types";
import {
  applyUrl,
  connectArgsOf,
  configuredTabs,
  connectionUrlOf,
  draftFromSaved,
  draftFromUrl,
  missingFields,
  newDraft,
  saveInputOf,
  savedTagOf,
  sshConfigOf,
  tabsFor,
  urlHints,
} from "../../src/lib/connectionDraft.ts";

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
  icon: "rocket",
  createdAt: "",
  updatedAt: "",
};

test("a new draft is empty with the default identity", () => {
  const d = newDraft("postgres");
  assert.equal(d.id, null);
  assert.deepEqual(d.identity, { name: "", color: "#3b82f6", icon: "" });
  assert.equal(d.fields.host, "");
});

test("a saved draft keeps its id", () => {
  const input = saveInputOf(draftFromSaved(saved));
  assert.equal(input.id, "c1");
  assert.equal(input.port, 5433);
  assert.equal(input.name, "Prod");
});

test("the save input carries the engine", () => {
  assert.equal(saveInputOf(draftFromSaved(saved)).engine, "postgres");
  assert.equal(saveInputOf(newDraft("postgres")).engine, "postgres");
});

test("applying a url keeps the id and identity", () => {
  const d = applyUrl(draftFromSaved(saved), "postgresql://u:p@other:6000/x");
  assert.ok(d);
  assert.equal(d.id, "c1");
  assert.deepEqual(d.identity, { name: "Prod", color: "#ef4444", icon: "rocket" });
  assert.equal(d.fields.host, "other");
  assert.equal(d.fields.port, "6000");
});

test("anything but a postgres url is not applied", () => {
  assert.equal(applyUrl(newDraft("postgres"), "mysql://u@h/db"), null);
  assert.equal(applyUrl(newDraft("postgres"), "localhost"), null);
});

test("host and database are required to connect, a name to save", () => {
  const d = newDraft("postgres");
  assert.deepEqual(missingFields(d, "connect").map((m) => m.fieldId), ["conn-host", "conn-db"]);
  assert.deepEqual(missingFields(d, "save").map((m) => m.fieldId), ["conn-name", "conn-host", "conn-db"]);
});

test("an ssh tunnel needs its host, user and, for keys, the key file", () => {
  const base = draftFromSaved(saved);
  const d = { ...base, sshEnabled: true, ssh: { ...base.ssh, authMethod: "key" as const } };
  assert.deepEqual(
    missingFields(d, "connect").map((m) => [m.tab, m.fieldId]),
    [
      ["ssh", "ssh-host"],
      ["ssh", "ssh-user"],
      ["ssh", "ssh-key"],
    ],
  );
});

test("optional tabs are configured only when turned on", () => {
  const d = draftFromSaved(saved);
  assert.deepEqual([...configuredTabs(d)], []);
  const on = { ...d, sshEnabled: true, fields: { ...d.fields, ssl: true, readOnly: true } };
  assert.deepEqual([...configuredTabs(on)].sort(), ["options", "ssh", "ssl"]);
});

test("socket hosts ignore ssl and ssh", () => {
  const base = draftFromSaved(saved);
  const d = { ...base, sshEnabled: true, fields: { ...base.fields, host: "/tmp", ssl: true } };
  assert.deepEqual([...configuredTabs(d)], []);
  assert.deepEqual(missingFields(d, "connect"), []);
  assert.equal(sshConfigOf(d), null);
});

test("the connection url and saved tag come from the draft", () => {
  const d = draftFromSaved(saved);
  assert.equal(connectionUrlOf(d), "postgresql://me:pw@db.example.com:5433/app");
  assert.deepEqual(savedTagOf(d), { name: "Prod", color: "#ef4444", icon: "rocket" });
  assert.equal(savedTagOf(newDraft("postgres")), undefined);
});

const savedSqlite: SavedConnection = {
  ...saved,
  id: "s1",
  name: "Notes",
  engine: "sqlite",
  host: "",
  port: 0,
  database: "",
  user: "",
  password: "",
  path: "/Users/me/notes.sqlite",
  readOnly: true,
};

test("a new sqlite draft has no file yet", () => {
  const d = newDraft("sqlite");
  assert.equal(d.engine, "sqlite");
  assert.equal(d.sqlitePath, "");
  assert.equal(d.sqliteCreate, false);
  assert.deepEqual(tabsFor("sqlite"), ["file", "options"]);
});

test("sqlite drafts need a file", () => {
  const d = newDraft("sqlite");
  assert.deepEqual(missingFields(d, "connect").map((m) => [m.tab, m.fieldId]), [["file", "sqlite-path"]]);
  assert.deepEqual(missingFields(d, "save").map((m) => m.fieldId), ["conn-name", "sqlite-path"]);
  assert.deepEqual(missingFields({ ...d, sqlitePath: "  " }, "connect").map((m) => m.fieldId), ["sqlite-path"]);
  assert.deepEqual(missingFields({ ...d, sqlitePath: "/x.db" }, "connect"), []);
});

test("sqlite drafts ignore the server fields", () => {
  const d = { ...newDraft("sqlite"), sqlitePath: "/x.db", sshEnabled: true };
  assert.deepEqual(missingFields(d, "connect"), []);
  assert.equal(sshConfigOf(d), null);
});

test("sqlite drafts connect with their path and options", () => {
  const d = { ...newDraft("sqlite"), sqlitePath: " /x.db ", sqliteCreate: true, fields: { ...newDraft("sqlite").fields, readOnly: true } };
  assert.deepEqual(connectArgsOf(d), { kind: "sqlite", path: "/x.db", readOnly: true, create: true });
  assert.deepEqual(connectArgsOf(draftFromSaved(saved)), { kind: "postgres", url: "postgresql://me:pw@db.example.com:5433/app", ssh: null });
});

test("only the read-only option makes a sqlite tab configured", () => {
  const d = { ...newDraft("sqlite"), sqlitePath: "/x.db" };
  assert.deepEqual([...configuredTabs(d)], []);
  assert.deepEqual([...configuredTabs({ ...d, fields: { ...d.fields, readOnly: true } })], ["options"]);
});

test("a saved sqlite connection becomes a sqlite draft", () => {
  const d = draftFromSaved(savedSqlite);
  assert.equal(d.engine, "sqlite");
  assert.equal(d.id, "s1");
  assert.equal(d.sqlitePath, "/Users/me/notes.sqlite");
  assert.equal(d.sqliteCreate, false);
  assert.equal(d.fields.readOnly, true);
  assert.equal(d.identity.name, "Notes");
});

test("saving a sqlite draft keeps the path and sends neutral postgres fields", () => {
  const input = saveInputOf(draftFromSaved(savedSqlite));
  assert.equal(input.engine, "sqlite");
  assert.equal(input.path, "/Users/me/notes.sqlite");
  assert.equal(input.id, "s1");
  assert.equal(input.readOnly, true);
  assert.deepEqual(
    [input.host, input.port, input.database, input.user, input.password, input.ssl, input.tls, input.ssh],
    ["", 0, "", "", "", false, null, null],
  );
});

test("saving a postgres draft sends no path", () => {
  assert.equal(saveInputOf(draftFromSaved(saved)).path, "");
});

test("applying a postgres url never changes a sqlite draft", () => {
  assert.equal(applyUrl(newDraft("sqlite"), "postgresql://u:p@h:5432/db"), null);
});

const savedMysql: SavedConnection = {
  ...saved,
  id: "m1",
  name: "Shop",
  engine: "mysql",
  host: "db.local",
  port: 3307,
  database: "",
  user: "ana",
  password: "pw",
  tls: { mode: "verify-ca", rootCert: "/ca.pem", cert: "", key: "" },
  ssl: true,
  readOnly: true,
};

test("a mysql draft starts on port 3306 and doesn't require a database", () => {
  const d = newDraft("mysql");
  assert.equal(d.fields.port, "3306");
  assert.deepEqual(missingFields({ ...d, fields: { ...d.fields, host: "db.local" } }, "connect"), []);
  assert.deepEqual(missingFields(d, "connect"), [{ tab: "connection", fieldId: "conn-host" }]);
  // PostgreSQL still wants the database
  const pg = newDraft("postgres");
  assert.deepEqual(missingFields({ ...pg, fields: { ...pg.fields, host: "h" } }, "connect"), [{ tab: "connection", fieldId: "conn-db" }]);
});

test("a mysql url fills the form", () => {
  const d = applyUrl(newDraft("mysql"), "mysql://ana:s%40l@db.local:3307/shop?ssl-mode=VERIFY_CA&ssl-ca=%2Fca.pem")!;
  const { host, port, user, password, database, ssl, sslMode, sslRootCert } = d.fields;
  assert.deepEqual({ host, port, user, password, database, ssl, sslMode, sslRootCert }, {
    host: "db.local",
    port: "3307",
    user: "ana",
    password: "s@l",
    database: "shop",
    ssl: true,
    sslMode: "verify-ca",
    sslRootCert: "/ca.pem",
  });
  assert.equal(applyUrl(newDraft("mysql"), "mysql://db.local/shop?ssl-mode=VERIFY_IDENTITY")!.fields.sslMode, "verify-full");
  assert.equal(applyUrl(newDraft("mysql"), "mysql://db.local/shop?ssl-mode=REQUIRED")!.fields.sslMode, "require");
  const plain = applyUrl(newDraft("mysql"), "mysql://db.local")!.fields;
  assert.deepEqual([plain.port, plain.database, plain.ssl, plain.user], ["3306", "", false, ""]);
  assert.equal(applyUrl(newDraft("mysql"), "mysql://db.local/shop?ssl-mode=DISABLED")!.fields.ssl, false);
  assert.equal(applyUrl(newDraft("mysql"), "mysql://db.local/shop?ssl-mode=PREFERRED")!.fields.ssl, false);
});

test("a url only fills the form of its own database", () => {
  assert.equal(applyUrl(newDraft("mysql"), "postgresql://me@db.local/app"), null);
  assert.equal(applyUrl(newDraft("postgres"), "mysql://me@db.local/app"), null);
  assert.equal(applyUrl(newDraft("mysql"), "not a url"), null);
});

test("mysql connects with its fields, not with a url", () => {
  assert.deepEqual(connectArgsOf(draftFromSaved(savedMysql)), {
    kind: "mysql",
    params: {
      host: "db.local",
      port: 3307,
      database: "",
      user: "ana",
      password: "pw",
      readOnly: true,
      tls: { mode: "verify-ca", rootCert: "/ca.pem", cert: "", key: "" },
    },
    ssh: null,
  });
  const noTls = connectArgsOf(draftFromSaved({ ...savedMysql, ssl: false, tls: null, readOnly: false }));
  assert.equal(noTls.kind === "mysql" && noTls.params.tls, null);
  const tunnelled = connectArgsOf({ ...draftFromSaved(savedMysql), sshEnabled: true, ssh: { host: "bastion", port: "2200", user: "u", authMethod: "agent", password: "", keyPath: "", passphrase: "" } });
  assert.equal(tunnelled.kind === "mysql" && tunnelled.ssh?.port, 2200);
});

test("a saved mysql connection reads back as a mysql draft and saves as one", () => {
  const d = draftFromSaved(savedMysql);
  assert.equal(d.engine, "mysql");
  assert.equal(d.fields.port, "3307");
  const input = saveInputOf(d);
  assert.deepEqual([input.engine, input.id, input.port, input.database, input.ssl, input.readOnly, input.tls?.mode], ["mysql", "m1", 3307, "", true, true, "verify-ca"]);
  // No port typed means MySQL's own
  assert.equal(saveInputOf({ ...d, fields: { ...d.fields, port: "" } }).port, 3306);
});

test("mysql has the server tabs, and ssh follows the draft", () => {
  assert.deepEqual(tabsFor("mysql"), ["connection", "ssl", "ssh", "options"]);
  const d = { ...newDraft("mysql"), sshEnabled: true, ssh: { host: "b", port: "", user: "u", authMethod: "agent" as const, password: "", keyPath: "", passphrase: "" } };
  assert.deepEqual([...configuredTabs({ ...d, fields: { ...d.fields, host: "db" } })], ["ssh"]);
  assert.equal(sshConfigOf({ ...d, fields: { ...d.fields, host: "db" } })?.host, "b");
  assert.equal(sshConfigOf({ ...d, fields: { ...d.fields, host: "/var/run/mysqld" } }), null);
});

test("a url opens the draft of the database it names", () => {
  const mysql = draftFromUrl("mysql://ana:pw@db.local:3307/shop?ssl-mode=REQUIRED")!;
  assert.deepEqual([mysql.engine, mysql.fields.host, mysql.fields.port, mysql.fields.database, mysql.fields.ssl], ["mysql", "db.local", "3307", "shop", true]);
  const pg = draftFromUrl("postgresql://me:pw@db.example.com/app")!;
  assert.deepEqual([pg.engine, pg.fields.host, pg.fields.port, pg.fields.database], ["postgres", "db.example.com", "5432", "app"]);
  assert.equal(draftFromUrl("postgres://me@h/app")?.engine, "postgres");
  assert.equal(draftFromUrl("  mysql://h  ")?.engine, "mysql");
  assert.equal(draftFromUrl("sqlite:///x.db"), null);
  assert.equal(draftFromUrl("not a url"), null);
  assert.equal(draftFromUrl(""), null);
});

test("the url dialog speaks of the database it is for", () => {
  assert.deepEqual(urlHints("mysql"), {
    description: "Paste a MySQL connection string to fill in the form.",
    placeholder: "mysql://user:password@host:3306/database",
    invalid: "That doesn't look like a mysql:// connection URL.",
  });
  assert.equal(urlHints("postgres").placeholder, "postgresql://user:password@host:5432/database");
  assert.equal(urlHints("postgres").invalid, "That doesn't look like a postgres:// connection URL.");
  // From the home page any server database will do
  assert.equal(urlHints(undefined).description, "Paste a PostgreSQL or MySQL connection string to fill in the form.");
  assert.equal(urlHints(undefined).invalid, "That doesn't look like a postgres:// or mysql:// connection URL.");
});
