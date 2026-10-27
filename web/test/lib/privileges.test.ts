import { test } from "node:test";
import assert from "node:assert/strict";
import { BASE_PRIVILEGES, privilegeColumns } from "../../src/lib/privileges.ts";
import type { TablePrivilege } from "../../src/types/index.ts";

const grant = (grantee: string, privileges: string[]) => ({ grantee, privileges }) as TablePrivilege;

test("maintain is offered where the server has it", () => {
  assert.deepEqual(privilegeColumns(true, []), [...BASE_PRIVILEGES, "MAINTAIN"]);
  assert.deepEqual(privilegeColumns(false, [grant("app", ["SELECT"])]), BASE_PRIVILEGES);
});

test("existing maintain grants keep the column", () => {
  assert.deepEqual(privilegeColumns(false, [grant("app", ["SELECT", "MAINTAIN"])]), [...BASE_PRIVILEGES, "MAINTAIN"]);
});
