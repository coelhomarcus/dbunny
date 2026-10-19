import { useEffect, useMemo, useRef, useState } from "react";
import { FileCode, Plus, Save, X } from "lucide-react";
import type { RoleInfo, StructureChange, TablePrivilege } from "@/types";
import { accountOf } from "../../lib/roles";
import { useApi, useConnection } from "../../contexts/useConnection";
import { cn } from "../../lib/cn";
import Button from "../ui/Button";
import Checkbox from "../Checkbox";
import Popover from "../ui/Popover";
import Tooltip from "../Tooltip";

const BASE_PRIVILEGES = ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER"];

const th = "sticky top-0 z-[1] bg-card-header border-b border-border px-4 py-2.5 text-left text-[13px] font-medium text-muted-foreground whitespace-nowrap";
const td = "border-b border-border px-4 py-2 align-middle";

type Rights = Record<string, string[]>;

/** The GRANT and REVOKE statements that turn `from` into `to`. */
function diff(from: Rights, to: Rights): StructureChange[] {
  const changes: StructureChange[] = [];
  for (const grantee of new Set([...Object.keys(from), ...Object.keys(to)])) {
    const before = from[grantee] ?? [];
    const after = to[grantee] ?? [];
    const granted = after.filter((p) => !before.includes(p));
    const revoked = before.filter((p) => !after.includes(p));
    if (granted.length) changes.push({ kind: "grant", grantee, privileges: granted, grantOption: false });
    if (revoked.length) changes.push({ kind: "revoke", grantee, privileges: revoked });
  }
  return changes;
}

/**
 * Who may do what on the table, as a grid of checkboxes. Ticks are collected and run
 * together as GRANT/REVOKE statements, shown before they run.
 */
export default function PermissionsEditor({
  schema,
  table,
  owner,
  privileges,
  canEdit,
  onDone,
}: {
  schema: string;
  table: string;
  owner: string;
  privileges: TablePrivilege[];
  canEdit: boolean;
  /** Called after the changes ran (or one failed, with its message), to read the permissions
   * again; the parent remounts the editor with them (keyed by the permissions). */
  onDone: (error: string | null) => void;
}) {
  const api = useApi();
  const serverVersion = useConnection().connectionInfo?.serverVersion ?? "";
  const original = useMemo<Rights>(() => Object.fromEntries(privileges.map((p) => [p.grantee, p.privileges])), [privileges]);
  const [rights, setRights] = useState<Rights>(original);
  const [roles, setRoles] = useState<RoleInfo[] | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const addRef = useRef<HTMLButtonElement>(null);
  const [sql, setSql] = useState<string | null>(null);
  const [showSql, setShowSql] = useState(false);
  const [saving, setSaving] = useState(false);

  // MAINTAIN exists from PostgreSQL 17 on
  const columns = useMemo(() => {
    const major = parseInt(serverVersion, 10);
    const hasMaintain = major >= 17 || privileges.some((p) => p.privileges.includes("MAINTAIN"));
    return hasMaintain ? [...BASE_PRIVILEGES, "MAINTAIN"] : BASE_PRIVILEGES;
  }, [serverVersion, privileges]);

  const changes = useMemo(() => diff(original, rights), [original, rights]);
  const changesKey = JSON.stringify(changes);

  useEffect(() => {
    if (!changes.length) return;
    let stale = false;
    Promise.all(changes.map((c) => api.previewStructureChange(schema, table, c)))
      .then((parts) => !stale && setSql(parts.join("\n")))
      .catch((err) => !stale && setSql(typeof err === "string" ? err : "Invalid change"));
    return () => {
      stale = true;
    };
    // changesKey stands for changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, schema, table, changesKey]);

  function toggle(grantee: string, privilege: string) {
    setRights((prev) => {
      const current = prev[grantee] ?? [];
      const next = current.includes(privilege) ? current.filter((p) => p !== privilege) : [...current, privilege];
      return { ...prev, [grantee]: next };
    });
  }

  function openAdd() {
    setAddOpen(true);
    if (!roles) api.getRoles().then((list) => setRoles(list.roles)).catch(() => setRoles([]));
  }

  function addGrantee(grantee: string) {
    setAddOpen(false);
    // A new role starts able to read, the usual first right to give
    setRights((prev) => ({ ...prev, [grantee]: prev[grantee]?.length ? prev[grantee] : ["SELECT"] }));
  }

  async function apply() {
    setSaving(true);
    let failure: string | null = null;
    try {
      for (const change of changes) await api.applyStructureChange(schema, table, change);
    } catch (err) {
      failure = typeof err === "string" ? err : "The change failed";
    }
    setSaving(false);
    onDone(failure);
  }

  // Rows: everyone with rights, plus roles added here; a role left with nothing stays until saved
  const grantees = [...new Set([...privileges.map((p) => p.grantee), ...Object.keys(rights)])];
  const grantable = Object.fromEntries(privileges.map((p) => [p.grantee, p.grantable]));
  const candidates = [
    ...(!grantees.includes("PUBLIC") ? ["PUBLIC"] : []),
    ...(roles ?? []).filter((r) => !r.isBuiltin && !grantees.includes(accountOf(r))).map(accountOf),
  ];

  return (
    <div className="flex h-full flex-col">
      {canEdit && (
        <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5">
          {changes.length > 0 ? (
            <>
              <span className="mr-auto text-xs tabular-nums text-muted-foreground">
                {changes.length} statement{changes.length > 1 ? "s" : ""} to run
              </span>
              <Button variant="ghost-muted" size="xs" data-active={showSql} onClick={() => setShowSql((v) => !v)}>
                <FileCode /> SQL
              </Button>
              <Button variant="ghost-muted" size="xs" onClick={() => setRights(original)}>
                <X /> Discard
              </Button>
              <Button variant="primary" size="xs" onClick={apply} disabled={saving}>
                <Save /> {saving ? "Applying…" : "Apply"}
              </Button>
            </>
          ) : (
            <>
              <Button ref={addRef} variant="ghost-muted" size="xs" className="ml-auto" onClick={openAdd}>
                <Plus /> Add role
              </Button>
            </>
          )}
        </div>
      )}
      {changes.length > 0 && showSql && sql && (
        <pre className="max-h-32 shrink-0 overflow-auto whitespace-pre-wrap break-words border-b border-border bg-muted px-4 py-2 font-mono text-xs">{sql}</pre>
      )}

      {grantees.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-muted-foreground">Nobody has rights on it</p>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full text-sm">
            <thead>
              <tr>
                <th className={th}>Role</th>
                {columns.map((p) => (
                  <th key={p} className={cn(th, "px-2 text-center text-[11px] tracking-wide")}>
                    {p}
                  </th>
                ))}
                <th className={th}>Can grant to others</th>
              </tr>
            </thead>
            <tbody className="[&>tr:nth-child(even)]:bg-muted [&>tr]:bg-card">
              {grantees.map((grantee) => {
                const isOwner = grantee === owner;
                const current = rights[grantee] ?? [];
                const before = original[grantee] ?? [];
                return (
                  <tr key={grantee}>
                    <td className={cn(td, "font-medium")}>
                      {grantee === "PUBLIC" ? <span className="italic">everyone (PUBLIC)</span> : grantee}
                      {isOwner && <span className="ml-2 text-[11px] font-normal text-muted-foreground">owner</span>}
                    </td>
                    {columns.map((p) => {
                      const checked = current.includes(p);
                      const changed = checked !== before.includes(p);
                      const box = (
                        <Checkbox
                          checked={checked}
                          onChange={() => toggle(grantee, p)}
                          disabled={!canEdit || isOwner}
                          className={cn(changed && "ring-2 ring-warning/50 ring-offset-1 ring-offset-card")}
                        />
                      );
                      return (
                        <td key={p} className={cn(td, "px-2")}>
                          <div className="flex justify-center">
                            {/* The owner's rights come with owning the table */}
                            {isOwner && canEdit ? (
                              <Tooltip label="The owner's rights come with owning the table" side="top">
                                {box}
                              </Tooltip>
                            ) : (
                              box
                            )}
                          </div>
                        </td>
                      );
                    })}
                    <td className={cn(td, "text-xs text-muted-foreground")}>
                      {grantable[grantee]?.length ? grantable[grantee].join(", ") : "—"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Popover open={addOpen} onClose={() => setAddOpen(false)} anchorRef={addRef} className="w-56 p-1">
        <div className="max-h-64 overflow-y-auto">
          {roles === null ? (
            <p className="px-2 py-2 text-xs text-muted-foreground">Loading roles…</p>
          ) : candidates.length === 0 ? (
            <p className="px-2 py-2 text-xs text-muted-foreground">Every role is already listed</p>
          ) : (
            candidates.map((name) => (
              <button
                key={name}
                type="button"
                onClick={() => addGrantee(name)}
                className="flex w-full items-center rounded-md px-2 py-1.5 text-left text-[13px] cursor-pointer hover:bg-accent"
              >
                {name === "PUBLIC" ? <span className="italic">everyone (PUBLIC)</span> : name}
              </button>
            ))
          )}
        </div>
      </Popover>
    </div>
  );
}
