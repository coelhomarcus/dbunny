import { useRef, useState } from "react";
import { Check, ChevronsUpDown, Database, Loader, Plus, Trash2 } from "lucide-react";
import type { DatabaseInfo } from "@/types";
import { useConnection } from "../../contexts/useConnection";
import { useTab, useWorkspace } from "../../contexts/workspace";
import { formatBytes } from "../../lib/format";
import { cn } from "../../lib/cn";
import Popover from "../ui/Popover";
import ContainerDialog, { type ContainerOptions, type ContainerRequest } from "../schema/ContainerDialog";
import { useCapabilities } from "../../hooks/useCapabilities";

/**
 * The connection name at the top of the sidebar, with the database in use under it; opens
 * the other databases on the same server. Picking one opens it in a new tab with the same
 * login, SSL, SSH and read-only settings.
 */
export default function DatabaseSwitcher() {
  const { api, connectionInfo, savedConnectionName } = useConnection();
  const { tabs, openTab } = useWorkspace();
  const { tabId } = useTab();
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [databases, setDatabases] = useState<DatabaseInfo[] | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [container, setContainer] = useState<ContainerRequest | null>(null);
  const readOnly = connectionInfo?.readOnly ?? false;
  const capabilities = useCapabilities();

  function loadDatabases() {
    setError(null);
    api.getDatabases().then(setDatabases).catch((err) => setError(typeof err === "string" ? err : "Couldn't list databases"));
  }

  function toggle() {
    if (!open) loadDatabases();
    setOpen(!open);
  }

  function ask(request: ContainerRequest) {
    setOpen(false);
    setContainer(request);
  }

  async function runContainer(request: ContainerRequest, options: ContainerOptions) {
    if (request.action === "create") {
      await api.createDatabase(options.name);
      // The new database opens right away; its errors show in the list next time
      openDatabase(options.name);
    } else {
      await api.dropDatabase(request.name, options.force);
      setDatabases((prev) => prev?.filter((db) => db.name !== request.name) ?? prev);
    }
  }

  async function openDatabase(name: string) {
    setOpening(name);
    setError(null);
    try {
      const response = await api.connectDatabase(name);
      const tab = tabs.find((t) => t.id === tabId);
      openTab({
        response,
        saved: { name: savedConnectionName ? `${savedConnectionName} / ${name}` : name, color: tab?.color ?? null, icon: tab?.icon ?? null },
      });
      setOpen(false);
    } catch (err) {
      setError(typeof err === "string" ? err : "Couldn't open the database");
    } finally {
      setOpening(null);
    }
  }

  const database = connectionInfo?.database ?? "";
  // A tab opened from here is named "Connection / database": the database has its own line
  const connectionName = savedConnectionName?.endsWith(` / ${database}`)
    ? savedConnectionName.slice(0, -(database.length + 3))
    : savedConnectionName;

  // An engine with a single database has nothing to switch to: just say which one this is
  if (!capabilities.databases) {
    return (
      <div className="flex min-w-0 flex-1 items-center rounded-md border border-border px-2 py-1">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium leading-tight">{connectionName || "Connection"}</div>
          <div className="flex min-w-0 items-center gap-1 text-[11px] leading-tight text-muted-foreground">
            <Database size={10} className="shrink-0" />
            <span className="truncate">{database}</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="relative min-w-0 flex-1">
      <button
        ref={anchorRef}
        type="button"
        onClick={toggle}
        aria-expanded={open}
        aria-label="Switch database"
        className="group flex w-full min-w-0 items-center gap-1.5 rounded-md border border-border px-2 py-1 text-left cursor-pointer transition-colors hover:bg-accent aria-expanded:bg-accent"
      >
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium leading-tight">{connectionName || "Connection"}</div>
          <div className="flex min-w-0 items-center gap-1 text-[11px] leading-tight text-muted-foreground">
            <Database size={10} className="shrink-0" />
            <span className="truncate">{database}</span>
          </div>
        </div>
        <ChevronsUpDown size={12} className="shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground" />
      </button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} dock className="p-1">
        <div className="px-2 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Databases on this server</div>
        {databases === null && !error ? (
          <div className="flex justify-center py-3">
            <Loader size={14} className="animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="max-h-72 overflow-y-auto">
            {(databases ?? []).map((db) => (
              <div key={db.name} className="group relative">
                <button
                  type="button"
                  disabled={db.isCurrent || !db.canConnect || opening !== null}
                  onClick={() => openDatabase(db.name)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] cursor-pointer hover:bg-accent disabled:cursor-default",
                    db.isCurrent ? "font-medium" : !db.canConnect && "opacity-50",
                    db.isCurrent && "hover:bg-transparent",
                    !readOnly && !db.isCurrent && "pr-8",
                  )}
                >
                  <Database size={13} className="shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate">{db.name}</span>
                  {opening === db.name ? (
                    <Loader size={12} className="shrink-0 animate-spin text-muted-foreground" />
                  ) : db.isCurrent ? (
                    <Check size={13} className="shrink-0 text-primary" />
                  ) : (
                    db.sizeBytes !== null && (
                      <span className={cn("shrink-0 text-[11px] tabular-nums text-muted-foreground", !readOnly && "group-hover:invisible")}>{formatBytes(db.sizeBytes)}</span>
                    )
                  )}
                </button>
                {/* The one in use can't be dropped from its own connection */}
                {!readOnly && !db.isCurrent && (
                  <button
                    type="button"
                    aria-label={`Drop database ${db.name}`}
                    onClick={() => ask({ kind: "database", action: "drop", name: db.name })}
                    className="absolute right-1.5 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity cursor-pointer hover:bg-destructive/10 hover:text-destructive-foreground group-hover:opacity-100 focus-visible:opacity-100"
                  >
                    <Trash2 size={12} />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
        {error && <p className="px-2 py-1.5 text-xs text-destructive-foreground">{error}</p>}
        <div className="mt-1 flex items-center gap-2 border-t border-border px-2 pb-0.5 pt-1.5">
          <p className="flex-1 text-[11px] text-muted-foreground">Opens in a new tab, with the same login.</p>
          {!readOnly && (
            <button
              type="button"
              onClick={() => ask({ kind: "database", action: "create", name: "" })}
              className="flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground cursor-pointer hover:bg-accent hover:text-foreground"
            >
              <Plus size={11} /> New
            </button>
          )}
        </div>
      </Popover>
      </div>
      <ContainerDialog request={container} onRun={(options) => runContainer(container!, options)} onClose={() => setContainer(null)} />
    </>
  );
}
