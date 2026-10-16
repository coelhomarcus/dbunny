import { useEffect, useState } from "react";
import { Users } from "lucide-react";
import type { RoleInfo } from "@/types";
import { useApi } from "../contexts/useConnection";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { TableSkeleton } from "../components/Skeleton";
import Badge from "../components/ui/Badge";
import Checkbox from "../components/Checkbox";
import { cn } from "../lib/cn";

/** Attributes worth a badge, in the order they're shown. */
const ATTRIBUTES: { key: keyof RoleInfo; label: string; variant: "destructive" | "warning" | "info" | "neutral" }[] = [
  { key: "superuser", label: "superuser", variant: "destructive" },
  { key: "createRole", label: "create role", variant: "warning" },
  { key: "createDb", label: "create db", variant: "info" },
  { key: "replication", label: "replication", variant: "neutral" },
  { key: "bypassRls", label: "bypass RLS", variant: "neutral" },
];

/** The server's roles: who can log in, what they may do and which groups they belong to. */
export default function RolesView() {
  const api = useApi();
  const [roles, setRoles] = useState<RoleInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showBuiltin, setShowBuiltin] = useState(false);

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

  const shown = (roles ?? []).filter((r) => showBuiltin || !r.isBuiltin);
  const builtinCount = (roles ?? []).filter((r) => r.isBuiltin).length;

  return (
    <div className="h-full flex flex-col">
      <PageHeader className="justify-between">
        <HeaderTitle label="Server" icon={<Users />}>
          Roles
        </HeaderTitle>
        {builtinCount > 0 && (
          <div className="flex cursor-pointer items-center gap-2 text-[13px] text-muted-foreground select-none" onClick={() => setShowBuiltin((v) => !v)}>
            <Checkbox checked={showBuiltin} onChange={() => setShowBuiltin((v) => !v)} />
            Show {builtinCount} built-in roles
          </div>
        )}
      </PageHeader>

      {error ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-destructive-foreground">{error}</div>
      ) : !roles ? (
        <TableSkeleton columns={5} rows={8} />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 z-[1] select-none">
              <tr className="text-[13px] text-muted-foreground [&>th]:border-b [&>th]:border-border [&>th]:bg-card-header [&>th]:px-4 [&>th]:py-2.5 [&>th]:text-left [&>th]:font-medium">
                <th>Role</th>
                <th>Login</th>
                <th>Rights</th>
                <th>Member of</th>
                <th>Connections</th>
                <th>Valid until</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((r, i) => (
                <tr key={r.name} className={cn("border-b border-border align-top", i % 2 ? "bg-muted" : "bg-card")}>
                  <td className="px-4 py-2 font-medium">
                    {r.name}
                    {r.isCurrent && <span className="ml-2 text-[11px] font-normal text-primary">you</span>}
                  </td>
                  <td className="px-4 py-2">{r.canLogin ? "yes" : <span className="text-muted-foreground">no (group)</span>}</td>
                  <td className="px-4 py-2">
                    <div className="flex flex-wrap gap-1">
                      {ATTRIBUTES.filter((a) => r[a.key]).map((a) => (
                        <Badge key={a.label} variant={a.variant}>
                          {a.label}
                        </Badge>
                      ))}
                    </div>
                  </td>
                  <td className="px-4 py-2 text-xs">{r.memberOf.length ? r.memberOf.join(", ") : <span className="text-muted-foreground">—</span>}</td>
                  <td className="px-4 py-2 tabular-nums text-muted-foreground">{r.connectionLimit < 0 ? "no limit" : r.connectionLimit}</td>
                  <td className="px-4 py-2 text-xs text-muted-foreground">{r.validUntil ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
