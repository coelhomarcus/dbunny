import { useState, useEffect, useRef } from "react";
import { useConnection } from "../contexts/useConnection";
import { useNavigate } from "react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
import { isMac } from "../lib/platform";
import type { SavedConnection } from "@/types";
import SavedConnectionsList from "../components/SavedConnectionsList";

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
      await connect(url, conn.name);
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
      const savedName = connectionName.trim() || undefined;
      await connect(url, savedName);
      navigate("/db/query");
    } catch {}
  }

  const canConnect = fields.host.trim() && fields.database.trim();
  const canSave = canConnect && connectionName.trim();
  const hasSavedConnections =
    savedConnections !== null && savedConnections.length > 0;

  return (
    <div className="h-full bg-zinc-950 flex flex-col">
      {isMac && (
        <div
          className="h-8 w-full shrink-0"
          onMouseDown={(e) => {
            e.preventDefault();
            getCurrentWindow().startDragging();
          }}
        />
      )}

      <div className="flex flex-1 overflow-hidden p-2 gap-2">
        <aside
          className={`shrink-0 bg-zinc-900 border border-zinc-800/60 rounded-xl shadow-xl flex flex-col select-none transition-all duration-300 ease-in-out overflow-hidden ${
            sidebarExpanded ? "w-64" : "w-12"
          }`}
        >
          {sidebarExpanded ? (
            <>
              <div className="h-11 px-3 flex items-center justify-between shrink-0">
                <span className="text-sm font-medium text-zinc-500 uppercase tracking-wider">
                  Connections
                </span>
                <div className="flex items-center gap-1">
                  {hasSavedConnections && (
                    <button
                      type="button"
                      onClick={handleNewConnection}
                      className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                      title="New connection"
                    >
                      <Plus size={14} />
                    </button>
                  )}
                  <button
                    onClick={() => setSidebarExpanded(false)}
                    className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                    title="Minimize sidebar"
                  >
                    <PanelLeftClose size={14} />
                  </button>
                </div>
              </div>
              <div className="flex-1 overflow-y-auto">
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
                    <p className="text-xs text-zinc-700 leading-relaxed">
                      {savedConnections === null
                        ? ""
                        : "Save a connection to access it here."}
                    </p>
                  </div>
                )}
              </div>
            </>
          ) : (
            <div className="flex flex-col items-center py-3 gap-3">
              <button
                onClick={() => setSidebarExpanded(true)}
                className="p-1.5 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                title="Expand sidebar"
              >
                <PanelLeftOpen size={16} />
              </button>
            </div>
          )}
        </aside>

        <main className="flex-1 flex items-center justify-center overflow-y-auto">
          <div className="w-full max-w-md">
            <div className="text-center mb-8 select-none">
              <div className="inline-flex items-center justify-center mb-3">
                <img src="/icon.svg" className="size-14" />
              </div>
              <h1 className="text-xl font-semibold text-white mb-1">
                {selectedId ? "Edit Connection" : "New Connection"}
              </h1>
              <p className="text-sm text-zinc-500">
                {selectedId
                  ? "Update your connection details"
                  : "Enter your PostgreSQL connection details"}
              </p>
            </div>

            <form onSubmit={handleSubmit} className="select-none">
              <div className="bg-zinc-900 rounded-xl border border-zinc-800 divide-y divide-zinc-800">
                <div className="flex divide-x divide-zinc-800">
                  <div className="flex-1 relative">
                    <label className="absolute top-2.5 left-10 text-[11px] font-medium text-zinc-500 uppercase tracking-wider select-none">
                      Host
                    </label>
                    <Server className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
                    <input
                      type="text"
                      value={fields.host}
                      onChange={(e) => updateField("host", e.target.value)}
                      placeholder="localhost"
                      className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors rounded-tl-xl"
                      spellCheck={false}
                      autoFocus
                    />
                  </div>
                  <div className="w-28">
                    <label className="absolute mt-2.5 ml-3 text-[11px] font-medium text-zinc-500 uppercase tracking-wider select-none">
                      Port
                    </label>
                    <input
                      type="text"
                      value={fields.port}
                      onChange={(e) => updateField("port", e.target.value)}
                      placeholder="5432"
                      className="w-full px-3 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors font-mono rounded-tr-xl"
                      spellCheck={false}
                    />
                  </div>
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-zinc-500 uppercase tracking-wider">
                    Database
                  </label>
                  <Database className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
                  <input
                    type="text"
                    value={fields.database}
                    onChange={(e) => updateField("database", e.target.value)}
                    placeholder="my_database"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-zinc-500 uppercase tracking-wider">
                    User
                  </label>
                  <User className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
                  <input
                    type="text"
                    value={fields.user}
                    onChange={(e) => updateField("user", e.target.value)}
                    placeholder="postgres"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-zinc-500 uppercase tracking-wider">
                    Password
                  </label>
                  <Lock className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
                  <input
                    type="password"
                    value={fields.password}
                    onChange={(e) => updateField("password", e.target.value)}
                    placeholder="••••••••"
                    className="w-full pl-10 pr-4 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors"
                    spellCheck={false}
                  />
                </div>

                <div className="flex items-center justify-between px-4 py-3">
                  <span className="text-sm text-zinc-400 select-none">
                    Require SSL
                  </span>
                  <button
                    type="button"
                    onClick={() => updateField("ssl", !fields.ssl)}
                    className={`relative w-9 h-5 rounded-full transition-colors ${
                      fields.ssl ? "bg-blue-600" : "bg-zinc-700"
                    }`}
                  >
                    <span
                      className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform ${
                        fields.ssl ? "translate-x-4" : ""
                      }`}
                    />
                  </button>
                </div>

                <div className="relative">
                  <label className="absolute top-2.5 left-10 text-[11px] font-medium text-zinc-500 uppercase tracking-wider select-none">
                    Name
                  </label>
                  <Bookmark className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
                  <input
                    type="text"
                    value={connectionName}
                    onChange={(e) => setConnectionName(e.target.value)}
                    placeholder="My Database"
                    className="w-full pl-10 pr-12 pt-7 pb-2.5 bg-transparent text-white text-sm placeholder-zinc-600 focus:outline-none focus:bg-zinc-800/50 transition-colors rounded-b-xl"
                    spellCheck={false}
                  />
                  <div className="absolute right-3.5 top-1/2 -translate-y-1/2">
                    <button
                      type="button"
                      onClick={() => colorInputRef.current?.click()}
                      className="w-5 h-5 rounded-full border-2 border-zinc-700 hover:border-zinc-500 transition-colors cursor-pointer"
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
                <button
                  type="button"
                  onClick={() => setShowImport(true)}
                  className="mt-3 flex items-center gap-1.5 text-xs text-zinc-500 hover:text-zinc-300 transition-colors cursor-pointer"
                >
                  <Link className="w-3.5 h-3.5" />
                  Import from URL
                </button>
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
                    className="flex-1 px-3 py-2 bg-zinc-900 border border-zinc-700 rounded-lg text-white text-xs placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-blue-500 font-mono"
                    autoFocus
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    onClick={handleImport}
                    className="px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-white text-xs rounded-lg transition-colors cursor-pointer"
                  >
                    Import
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setShowImport(false);
                      setImportUrl("");
                    }}
                    className="p-2 text-zinc-500 hover:text-zinc-300 transition-colors cursor-pointer"
                  >
                    <X className="w-4 h-4" />
                  </button>
                </div>
              )}

              {error && (
                <div className="mt-4 p-3 bg-red-950/50 border border-red-900/50 rounded-lg text-red-400 text-sm">
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
                    <button
                      type="button"
                      onClick={handleSave}
                      disabled={isSaving}
                      className="h-10 px-4 border border-zinc-700 hover:border-zinc-500 hover:bg-zinc-800/50 text-zinc-300 font-medium text-sm rounded-lg transition-colors cursor-pointer disabled:cursor-not-allowed relative whitespace-nowrap w-full"
                    >
                      <span className={`transition-opacity duration-200 ${isSaving ? "opacity-0" : "opacity-100"}`}>
                        {selectedId ? "Update" : "Save"}
                      </span>
                      <Loader className={`w-3.5 h-3.5 animate-spin absolute inset-0 m-auto transition-opacity duration-200 ${isSaving ? "opacity-100" : "opacity-0"}`} />
                    </button>
                  </div>
                </div>

                <button
                  type="submit"
                  disabled={!canConnect || isConnecting}
                  className="flex-1 h-10 bg-white hover:bg-zinc-200 disabled:bg-zinc-800 disabled:text-zinc-600 text-black font-medium text-sm rounded-lg transition-colors cursor-pointer disabled:cursor-not-allowed flex items-center justify-center gap-2"
                >
                  {isConnecting ? (
                    <>
                      <Loader className="w-4 h-4 animate-spin" />
                      Connecting...
                    </>
                  ) : (
                    <>
                      Connect
                      <ArrowRight className="w-4 h-4" />
                    </>
                  )}
                </button>
              </div>
            </form>
          </div>
        </main>
      </div>
    </div>
  );
}
