import { test } from "node:test";
import assert from "node:assert/strict";
import type { SavedConnection } from "@/types";
import {
  applyUrl,
  configuredTabs,
  connectionUrlOf,
  draftFromSaved,
  missingFields,
  newDraft,
  saveInputOf,
  savedTagOf,
  sshConfigOf,
} from "./connectionDraft.ts";

const saved: SavedConnection = {
  id: "c1",
  name: "Prod",
  engine: "postgres",
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
