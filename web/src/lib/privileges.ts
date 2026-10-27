import type { TablePrivilege } from "@/types";

/** The table privileges every supported server grants. */
export const BASE_PRIVILEGES = ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER"];

/**
 * The privilege columns of the permissions grid: MAINTAIN where the server has it, and also
 * where someone already holds it (a restored dump), so it can be revoked.
 */
export function privilegeColumns(maintainPrivilege: boolean, privileges: TablePrivilege[]): string[] {
  const held = privileges.some((p) => p.privileges.includes("MAINTAIN"));
  return maintainPrivilege || held ? [...BASE_PRIVILEGES, "MAINTAIN"] : BASE_PRIVILEGES;
}
