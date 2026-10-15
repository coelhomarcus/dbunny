import { useState, useEffect, useRef } from "react";
import { useConnection } from "../contexts/useConnection";
import { useNavigate } from "react-router";
import {
  Database,
  Server,
  User,
  Lock,
  Link,
  ArrowRight,
  X,
  Loader,
  Bookmark,
  Plus,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import {
  type ConnectionFields,
  parseConnectionUrl,
  buildConnectionUrl,
} from "../lib/connectionUrl";
import { api } from "../lib/api";
import type { SavedConnection } from "@/types";
import SavedConnectionsList from "../components/SavedConnectionsList";
import Tooltip from "../components/Tooltip";
import Button from "../components/ui/Button";
import { inputClass } from "../components/ui/styles";
import ThemeToggle from "../components/ui/ThemeToggle";
import { HeaderTitle } from "../components/layout/PageHeader";
import { cn } from "../lib/cn";

export default function ConnectPage() {
  const [fields, setFields] = useState<ConnectionFields>({
    host: "",
    port: "",
    database: "",
    user: "",
    password: "",
    ssl: false,
  });
  const [connectionName, setConnectionName] = useState("");
  const [connectionColor, setConnectionColor] = useState("#3b82f6");
  const [showImport, setShowImport] = useState(false);
  const [importUrl, setImportUrl] = useState("");
  const [savedConnections, setSavedConnections] = useState<
    SavedConnection[] | null
  >(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const colorInputRef = useRef<HTMLInputElement>(null);

  const [sidebarExpanded, setSidebarExpanded] = useState(true);
  const { connect, isConnecting, error } = useConnection();
  const navigate = useNavigate();

  useEffect(() => {
    loadSavedConnections();
  }, []);

  async function loadSavedConnections() {
    try {
      const connections = await api.getSavedConnections();
      setSavedConnections(connections);
    } catch {
      setSavedConnections([]);
    }
  }

  function updateField(key: keyof ConnectionFields, value: string | boolean) {
    setFields((prev) => ({ ...prev, [key]: value }));
  }

  function handleImport() {
    const parsed = parseConnectionUrl(importUrl);
    if (parsed.host) {
      setFields((prev) => ({ ...prev, ...parsed }));
      setShowImport(false);
      setImportUrl("");
    }
  }

  function handleSelectSaved(conn: SavedConnection) {
    setSelectedId(conn.id);
    setConnectionName(conn.name);
    setConnectionColor(conn.color || "#3b82f6");
    setFields({
      host: conn.host,
      port: conn.port.toString(),
      database: conn.database,
      user: conn.user,
      password: conn.password,
      ssl: conn.ssl,
    });
  }

  async function handleConnectSaved(conn: SavedConnection) {
    handleSelectSaved(conn);
    try {
      const url = buildConnectionUrl({
        host: conn.host,
        port: conn.port.toString(),
        database: conn.database,
        user: conn.user,
        password: conn.password,
        ssl: conn.ssl,
      });
      await connect(url, { name: conn.name, color: conn.color });
      navigate("/db/query");
    } catch {}
  }

  function handleNewConnection() {
    setSelectedId(null);
    setConnectionName("");
    setConnectionColor("#3b82f6");
    setFields({
      host: "",
      port: "",
      database: "",
      user: "",
      password: "",
      ssl: false,
    });
  }

  async function handleSave() {
    if (
      !connectionName.trim() ||
      !fields.host.trim() ||
      !fields.database.trim()
    )
      return;

    setIsSaving(true);
    try {
      await Promise.all([
        api.saveConnection({
          id: selectedId || undefined,
          name: connectionName.trim(),
          host: fields.host.trim(),
          port: parseInt(fields.port || "5432", 10),
          database: fields.database.trim(),
          user: fields.user.trim(),
          password: fields.password,
          ssl: fields.ssl,
          color: connectionColor,
        }),
        new Promise((r) => setTimeout(r, 600)),
      ]);
      await loadSavedConnections();
    } catch {} finally {
      setIsSaving(false);
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.deleteSavedConnection(id);
      if (selectedId === id) handleNewConnection();
      await loadSavedConnections();
    } catch {}
  }

  async function handleSubmit(e: { preventDefault: () => void }) {
    e.preventDefault();
    try {
      const url = buildConnectionUrl(fields);
      const savedName = connectionName.trim();
      await connect(url, savedName ? { name: savedName, color: connectionColor } : undefined);
      navigate("/db/query");
    } catch {}
  }

  const canConnect = fields.host.trim() && fields.database.trim();
  const canSave = canConnect && connectionName.trim();
  const hasSavedConnections =
    savedConnections !== null && savedConnections.length > 0;

  return (
    <div className="h-full bg-card flex flex-col">
      <div className="flex flex-1 overflow-hidden">
        <aside
          className={`shrink-0 border-r border-border flex flex-col select-none transition-[width] duration-300 ease-in-out overflow-hidden ${
            sidebarExpanded ? "w-64" : "w-12"
          }`}
        >
          <div className="relative flex-1">
            {/* Expanded view */}
            <div className={`absolute inset-y-0 left-0 w-64 flex flex-col transition-opacity duration-200 ${
              sidebarExpanded ? "opacity-100 delay-75" : "opacity-0 pointer-events-none"
            }`}>
              <div className="h-12 pl-3 pr-2 flex items-center justify-between shrink-0 border-b border-border">
                <HeaderTitle label="Saved">Connections</HeaderTitle>
                <div className="flex items-center gap-0.5">
                  {hasSavedConnections && (
                    <Button variant="ghost-muted" size="icon-sm" onClick={handleNewConnection} title="New connection">
                      <Plus />
                    </Button>
                  )}
                  <Button variant="ghost-muted" size="icon-sm" onClick={() => setSidebarExpanded(false)} title="Minimize sidebar">
                    <PanelLeftClose />
                  </Button>
                </div>
              </div>
              <div className="flex-1 overflow-y-auto py-1">
                {hasSavedConnections ? (
                  <SavedConnectionsList
                    connections={savedConnections}
                    selectedId={selectedId}
                    onSelect={handleSelectSaved}
                    onConnect={handleConnectSaved}
                    onDelete={handleDelete}
                  />
                ) : (
                  <div className="px-3 pt-4">
                    <p className="text-xs text-muted-foreground/70 leading-relaxed">
                      {savedConnections === null
                        ? ""
                        : "Save a connection to access it here."}
                    </p>
                  </div>
                )}
              </div>
              <div className="px-2 py-2 border-t border-border flex items-center">
                <ThemeToggle />
              </div>
            </div>

            {/* Collapsed view */}
            <div className={`absolute inset-0 flex flex-col items-center pb-2 transition-opacity duration-200 ${
              sidebarExpanded ? "opacity-0 pointer-events-none" : "opacity-100 delay-75"
            }`}>
              <div className="h-12 w-full shrink-0 flex items-center justify-center border-b border-border">
                <Tooltip label="Expand sidebar">
                  <Button variant="ghost-muted" size="icon-sm" onClick={() => setSidebarExpanded(true)} aria-label="Expand sidebar">
                    <PanelLeftOpen />
                  </Button>
                </Tooltip>
              </div>
              <div className="mt-auto">
                <ThemeToggle side="right" />
              </div>
            </div>
          </div>
        </aside>

        <main className="flex-1 flex items-center justify-center overflow-y-auto">
          <div className="w-full max-w-md">
            <div className="text-center mb-8 select-none">
              <div className="inline-flex items-center justify-center mb-3">
                <img src="/icon.svg" className="size-14 invert dark:invert-0" />
              </div>
              <h1 className="text-xl font-semibold text-foreground mb-1">
                {selectedId ? "Edit Connection" : "New Connection"}
              </h1>
              <p className="text-sm text-muted-foreground">
                {selectedId
                  ? "Update your connection details"
                  : "Enter your PostgreSQL connection details"}
              </p>
            </div>

            <form onSubmit={handleSubmit} className="select-none">
              <div className="bg-card rounded-xl border border-input shadow-xs/5 divide-y divide-border">
                <div className="flex divide-x divide-border">
                  <div className="flex-1 relative">
                    <label className="absolute top-2.5 left-10 text-[11px] font-medium text-muted-foreground uppercase tracking-wider select-none">
                      Host
                    </label>
                    <Server className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                    <input
                      type="text"
                      value={fields.host}
                      onChange={(e) => updateField("host", e.target.value)}
                      placeholder="localhost"
                      className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors rounded-tl-xl"
                      spellCheck={false}
                      autoFocus
                    />
                  </div>
                  <div className="w-28">
                    <label className="absolute mt-2.5 ml-3 text-[11px] font-medium text-muted-foreground uppercase tracking-wider select-none">
                      Port
                    </label>
                    <input
                      type="text"
                      value={fields.port}
                      onChange={(e) => updateField("port", e.target.value)}
                      placeholder="5432"
                      className="w-full px-3 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors font-mono rounded-tr-xl"
                      spellCheck={false}
                    />
                  </div>
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                    Database
                  </label>
                  <Database className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                  <input
                    type="text"
                    value={fields.database}
                    onChange={(e) => updateField("database", e.target.value)}
                    placeholder="my_database"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                    User
                  </label>
                  <User className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                  <input
                    type="text"
                    value={fields.user}
                    onChange={(e) => updateField("user", e.target.value)}
                    placeholder="postgres"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                    Password
                  </label>
                  <Lock className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                  <input
                    type="password"
                    value={fields.password}
                    onChange={(e) => updateField("password", e.target.value)}
                    placeholder="••••••••"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="flex items-center justify-between px-4 py-3">
                  <span className="text-sm text-muted-foreground select-none">
                    Require SSL
                  </span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={fields.ssl}
                    onClick={() => updateField("ssl", !fields.ssl)}
                    className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer ${
                      fields.ssl ? "bg-primary" : "bg-foreground/15"
                    }`}
                  >
                    <span
                      className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow-xs transition-transform ${
                        fields.ssl ? "translate-x-4" : ""
                      }`}
                    />
                  </button>
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-muted-foreground uppercase tracking-wider select-none">
                    Name
                  </label>
                  <Bookmark className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                  <input
                    type="text"
                    value={connectionName}
                    onChange={(e) => setConnectionName(e.target.value)}
                    placeholder="My Database"
                    className="w-full pl-10 pr-12 pt-7 pb-2.5 bg-transparent text-foreground text-sm placeholder:text-muted-foreground/70 focus:outline-none focus:bg-muted transition-colors rounded-b-xl"
                    spellCheck={false}
                  />
                  <div className="absolute right-3.5 top-1/2 -translate-y-1/2">
                    <button
                      type="button"
                      onClick={() => colorInputRef.current?.click()}
                      className="w-5 h-5 rounded-full border-2 border-input hover:border-ring transition-colors cursor-pointer"
                      style={{ backgroundColor: connectionColor }}
                      title="Pick a color"
                    />
                    <input
                      ref={colorInputRef}
                      type="color"
                      value={connectionColor}
                      onChange={(e) => setConnectionColor(e.target.value)}
                      className="sr-only"
                      tabIndex={-1}
                    />
                  </div>
                </div>
              </div>

              {!showImport ? (
                <Button variant="ghost-muted" size="xs" className="mt-2 -ml-2" onClick={() => setShowImport(true)}>
                  <Link />
                  Import from URL
                </Button>
              ) : (
                <div className="mt-3 flex gap-2">
                  <input
                    type="text"
                    value={importUrl}
                    onChange={(e) => setImportUrl(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        handleImport();
                      }
                      if (e.key === "Escape") {
                        setShowImport(false);
                        setImportUrl("");
                      }
                    }}
                    placeholder="postgresql://user:pass@host:5432/db"
                    className={cn(inputClass, "flex-1 text-xs font-mono")}
                    autoFocus
                    spellCheck={false}
                  />
                  <Button variant="outline" onClick={handleImport}>
                    Import
                  </Button>
                  <Button
                    variant="ghost-muted"
                    size="icon"
                    aria-label="Cancel import"
                    onClick={() => {
                      setShowImport(false);
                      setImportUrl("");
                    }}
                  >
                    <X />
                  </Button>
                </div>
              )}

              {error && (
                <div className="mt-4 p-3 bg-destructive/10 border border-destructive/30 rounded-lg text-destructive-foreground text-sm">
                  {error}
                </div>
              )}

              <div className="mt-4 flex">
                <div
                  className="shrink-0 overflow-hidden transition-[max-width,opacity] duration-500"
                  style={{
                    maxWidth: canSave ? "140px" : "0px",
                    opacity: canSave ? 1 : 0,
                  }}
                >
                  <div className="pr-2">
                    <Button variant="outline" size="lg" onClick={handleSave} disabled={isSaving} className="w-full">
                      <span className={`transition-opacity duration-200 ${isSaving ? "opacity-0" : "opacity-100"}`}>
                        {selectedId ? "Update" : "Save"}
                      </span>
                      <Loader className={`size-3.5 animate-spin absolute inset-0 m-auto transition-opacity duration-200 ${isSaving ? "opacity-100" : "opacity-0"}`} />
                    </Button>
                  </div>
                </div>

                <Button type="submit" variant="primary" size="lg" disabled={!canConnect || isConnecting} className="flex-1">
                  {isConnecting ? (
                    <>
                      <Loader className="animate-spin" />
                      Connecting...
                    </>
                  ) : (
                    <>
                      Connect
                      <ArrowRight />
                    </>
                  )}
                </Button>
              </div>
            </form>
          </div>
        </main>
      </div>
    </div>
  );
}
