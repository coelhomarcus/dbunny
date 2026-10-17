import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Copy, Users, X } from "lucide-react";
import ContextMenu from "../components/ui/ContextMenu";
import { writeClipboardText } from "../lib/clipboard";
import type { RoleInfo } from "@/types";
import { useApi } from "../contexts/useConnection";
import { useRightPanel } from "../contexts/rightPanel";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import SideCard from "../components/layout/SideCard";
import { TableSkeleton } from "../components/Skeleton";
import Badge from "../components/ui/Badge";
import Button from "../components/ui/Button";
import SegmentedTabs from "../components/ui/SegmentedTabs";
import { cn } from "../lib/cn";

/** Attributes worth a badge, in the order they're shown. */
const ATTRIBUTES: { key: keyof RoleInfo; label: string; variant: "destructive" | "warning" | "info" | "neutral" }[] = [
  { key: "superuser", label: "superuser", variant: "destructive" },
  { key: "createRole", label: "create role", variant: "warning" },
  { key: "createDb", label: "create db", variant: "info" },
  { key: "replication", label: "replication", variant: "neutral" },
  { key: "bypassRls", label: "bypass RLS", variant: "neutral" },
];

function rightsOf(role: RoleInfo) {
  return ATTRIBUTES.filter((attribute) => role[attribute.key]);
}

/** The server's roles: who can log in, what they may do and which groups they belong to. */
export default function RolesView() {
  const api = useApi();
  const rightPanel = useRightPanel();
  const [roles, setRoles] = useState<RoleInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showBuiltin, setShowBuiltin] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; name: string } | null>(null);
  const [shownRole, setShownRole] = useState<RoleInfo | null>(null);

  useEffect(() => {
    let stale = false;
    api
      .getRoles()
      .then((list) => !stale && setRoles(list))
      .catch((err) => !stale && setError(typeof err === "string" ? err : "Couldn't read the roles"));
    return () => {
      stale = true;
    };
  }, [api]);

  const shown = (roles ?? []).filter((role) => showBuiltin || !role.isBuiltin);
  const builtinCount = (roles ?? []).filter((role) => role.isBuiltin).length;

  useEffect(() => {
    if (!selected || !roles) return;
    const role = roles.find((item) => item.name === selected);
    if (!role || (!showBuiltin && role.isBuiltin)) setSelected(null);
    else setShownRole(role);
  }, [roles, selected, showBuiltin]);

  return (
    <div className="flex h-full flex-col">
      <PageHeader className="justify-between gap-2">
        <HeaderTitle label="Server" icon={<Users />}>
          Roles
        </HeaderTitle>
        {builtinCount > 0 && (
          <SegmentedTabs
            value={showBuiltin ? "all" : "roles"}
            onChange={(value) => setShowBuiltin(value === "all")}
            options={[
              { value: "roles", label: "Roles" },
              { value: "all", label: "All" },
            ]}
          />
        )}
      </PageHeader>

      {error ? (
        <div className="flex flex-1 select-text items-center justify-center p-4 text-sm text-destructive-foreground">{error}</div>
      ) : !roles ? (
        <TableSkeleton columns={4} rows={8} />
      ) : (
        <>
          <div className="min-h-0 flex-1 overflow-auto">
            <table className="w-full table-fixed text-sm">
              <colgroup>
                <col className="w-[14rem]" />
                <col className="w-[6.5rem]" />
                <col />
                <col className="w-[16rem]" />
              </colgroup>
              <thead className="sticky top-0 z-[1] select-none">
                <tr className="text-[13px] text-muted-foreground [&>th]:border-b [&>th]:border-border [&>th]:bg-card-header [&>th]:px-4 [&>th]:py-2.5 [&>th]:text-left [&>th]:font-medium [&>th]:whitespace-nowrap">
                  <th>Role</th>
                  <th>Login</th>
                  <th>Rights</th>
                  <th>Member of</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((role, index) => {
                  const rights = rightsOf(role);
                  const active = selected === role.name;
                  return (
                    <tr
                      key={role.name}
                      onClick={() => setSelected(active ? null : role.name)}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setMenu({ x: e.clientX, y: e.clientY, name: role.name });
                      }}
                      className={cn(
                        "cursor-pointer border-b border-border align-middle transition-colors hover:bg-accent",
                        active ? "bg-accent" : index % 2 ? "bg-muted/50" : "bg-card",
                      )}
                    >
                      <td className="truncate px-4 py-2 font-medium">
                        {role.name}
                        {role.isCurrent && <span className="ml-2 text-[11px] font-normal text-primary">you</span>}
                      </td>
                      <td className="px-4 py-2">
                        <Badge variant={role.canLogin ? "success" : "neutral"}>{role.canLogin ? "login" : "group"}</Badge>
                      </td>
                      <td className="px-4 py-2">
                        {rights.length === 0 ? (
                          <span className="text-muted-foreground">—</span>
                        ) : (
                          <div className="flex flex-nowrap gap-1 overflow-hidden">
                            {rights.map((attribute) => (
                              <Badge key={attribute.label} variant={attribute.variant} className="shrink-0">
                                {attribute.label}
                              </Badge>
                            ))}
                          </div>
                        )}
                      </td>
                      <td className="truncate px-4 py-2 text-xs text-muted-foreground">
                        {role.memberOf.length ? role.memberOf.join(", ") : "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {shown.length === 0 && <p className="px-4 py-10 text-center text-sm text-muted-foreground">No roles.</p>}
          </div>

          <footer className="flex h-9 shrink-0 items-center border-t border-border bg-card-header px-4 text-[12px] text-muted-foreground">
            {shown.length} role{shown.length === 1 ? "" : "s"}
            {!showBuiltin && builtinCount > 0 && <span className="text-muted-foreground/80"> · {builtinCount} built-in hidden</span>}
          </footer>
        </>
      )}

      <ContextMenu
        position={menu}
        items={menu ? [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(menu.name) }] : []}
        onClose={() => setMenu(null)}
      />
      {rightPanel && createPortal(
        <SideCard open={selected !== null && shownRole !== null} widthClass="w-80" label={shownRole ? shownRole.name : "Role"}>
          {shownRole && <RolePanel role={shownRole} onClose={() => setSelected(null)} />}
        </SideCard>,
        rightPanel,
      )}
    </div>
  );
}

function RolePanel({ role, onClose }: { role: RoleInfo; onClose: () => void }) {
  const rights = rightsOf(role);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium">{role.name}</span>
          {role.isCurrent && <span className="shrink-0 text-[11px] text-primary">you</span>}
        </div>
        <Button variant="ghost-muted" size="icon-sm" aria-label="Close" onClick={onClose}>
          <X />
        </Button>
      </header>

      <dl className="grid shrink-0 grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 border-b border-border px-3 py-2.5 text-[12px]">
        <dt className="text-muted-foreground">Login</dt>
        <dd>{role.canLogin ? "Can log in" : "Group, cannot log in"}</dd>
        <dt className="text-muted-foreground">Connections</dt>
        <dd>{role.connectionLimit < 0 ? "No limit" : role.connectionLimit}</dd>
        <dt className="text-muted-foreground">Valid until</dt>
        <dd>{role.validUntil ?? "—"}</dd>
        {role.isBuiltin && (
          <>
            <dt className="text-muted-foreground">Kind</dt>
            <dd>Built-in</dd>
          </>
        )}
      </dl>

      <div className="shrink-0 border-b border-border px-3 py-2.5">
        <p className="mb-1.5 text-[12px] text-muted-foreground">Rights</p>
        {rights.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">None</p>
        ) : (
          <div className="flex flex-wrap gap-1">
            {rights.map((attribute) => (
              <Badge key={attribute.label} variant={attribute.variant}>
                {attribute.label}
              </Badge>
            ))}
          </div>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-3 py-2.5">
        <p className="mb-1.5 text-[12px] text-muted-foreground">Member of</p>
        {role.memberOf.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">None</p>
        ) : (
          <ul className="flex flex-col gap-1 text-[13px]">
            {role.memberOf.map((name) => (
              <li key={name} className="truncate">
                {name}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
