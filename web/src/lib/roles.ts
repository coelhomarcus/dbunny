import type { RoleInfo } from "@/types";

/** A role the way a change names it: `name`, or `name@host` for an account that has a host (MySQL). */
export function accountOf(role: Pick<RoleInfo, "name" | "host">): string {
  return role.host ? `${role.name}@${role.host}` : role.name;
}

/** Whether any role has a host, which is when the page has to show them. */
export function hasHosts(roles: Pick<RoleInfo, "name" | "host">[]): boolean {
  return roles.some((role) => Boolean(role.host));
}
