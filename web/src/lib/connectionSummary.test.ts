import { test } from "node:test";
import assert from "node:assert/strict";
import type { SavedConnection } from "@/types";
import { mask, summaryOf } from "./connectionSummary.ts";

const saved: SavedConnection = {
  id: "c1",
  name: "Prod",
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
