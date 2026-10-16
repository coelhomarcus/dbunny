import { useState, useEffect, useCallback, type ComponentType } from "react";
import { useLocation, useNavigate, useParams } from "react-router";
import { useApi } from "../contexts/useConnection";
import type { SchemaInfo, TableInfo, FunctionInfo, SchemaObject, ExtensionInfo } from "@/types";
import type { SessionApi } from "../lib/api";
import {
  ChevronRight,
  Folder,
  Table2,
  Eye,
  Layers,
  Braces,
  ListOrdered,
  Shapes,
  Puzzle,
  Loader,
  type LucideProps,
} from "lucide-react";

type FolderKind = "tables" | "views" | "matviews" | "functions" | "sequences" | "types";
type FolderItem = TableInfo | FunctionInfo | SchemaObject;

interface FolderData {
  items: FolderItem[];
  loaded: boolean;
  loading: boolean;
}

/** One entry of a folder: how it looks and where it leads. */
interface ItemView {
  key: string;
  label: string;
  /** Small muted text after the label (e.g. a type's kind). */
  hint?: string;
  path: string;
}

interface FolderSpec {
  kind: FolderKind;
  label: string;
  /** Text colour class for the folder and its items' icons. */
  color: string;
  icon: ComponentType<LucideProps>;
  load: (api: SessionApi, schema: string) => Promise<FolderItem[]>;
  view: (item: FolderItem, schema: string) => ItemView;
}

const relationPath = (schema: string, name: string) => `/db/${encodeURIComponent(schema)}/${encodeURIComponent(name)}`;

const FOLDERS: FolderSpec[] = [
  {
    kind: "tables",
    label: "Tables",
    color: "text-success-foreground",
    icon: Table2,
    load: (api, schema) => api.getTables(schema),
    view: (item, schema) => ({ key: item.name, label: item.name, path: relationPath(schema, item.name) }),
  },
  {
    kind: "views",
    label: "Views",
    color: "text-info-foreground",
    icon: Eye,
    load: (api, schema) => api.getViews(schema),
    view: (item, schema) => ({ key: item.name, label: item.name, path: relationPath(schema, item.name) }),
  },
  {
    kind: "matviews",
    label: "Materialized views",
    color: "text-cyan-500",
    icon: Layers,
    load: (api, schema) => api.getMaterializedViews(schema),
    view: (item, schema) => ({ key: item.name, label: item.name, path: relationPath(schema, item.name) }),
  },
  {
    kind: "functions",
    label: "Functions",
    color: "text-purple-500",
    icon: Braces,
    load: (api, schema) => api.getFunctions(schema),
    view: (item, schema) => {
      const fn = item as FunctionInfo;
      return {
        key: `${fn.name}-${fn.argumentTypes}`,
        label: fn.name,
        path: `/db/${encodeURIComponent(schema)}/function/${encodeURIComponent(fn.name)}?args=${encodeURIComponent(fn.argumentTypes)}`,
      };
    },
  },
  {
    kind: "sequences",
    label: "Sequences",
    color: "text-teal-500",
    icon: ListOrdered,
    load: (api, schema) => api.getSequences(schema),
    view: (item, schema) => ({ key: item.name, label: item.name, path: `/db/${encodeURIComponent(schema)}/sequence/${encodeURIComponent(item.name)}` }),
  },
  {
    kind: "types",
    label: "Types",
    color: "text-pink-500",
    icon: Shapes,
    load: (api, schema) => api.getTypes(schema),
    view: (item, schema) => ({
      key: item.name,
      label: item.name,
      hint: (item as SchemaObject).kind,
      path: `/db/${encodeURIComponent(schema)}/type/${encodeURIComponent(item.name)}`,
    }),
  },
];

const EMPTY_FOLDER: FolderData = { items: [], loaded: false, loading: false };

interface SchemaTreeProps {
  schemaToExpand?: string | null;
  onSchemaExpanded?: () => void;
}

export default function SchemaTree({ schemaToExpand, onSchemaExpanded }: SchemaTreeProps = {}) {
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [folderData, setFolderData] = useState<Record<string, FolderData>>({});
  const [extensions, setExtensions] = useState<ExtensionInfo[] | null>(null);
  const [loading, setLoading] = useState(true);
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams();
  const api = useApi();

  useEffect(() => {
    api
      .getSchemas()
      .then((s) => {
        setSchemas(s);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [api]);

  useEffect(() => {
    if (!schemaToExpand) return;
    setExpanded((prev) => new Set([...prev, schemaToExpand]));
    onSchemaExpanded?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schemaToExpand]);

  const toggle = useCallback((key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const loadFolder = useCallback(
    async (schema: string, spec: FolderSpec) => {
      const key = `${schema}:${spec.kind}`;
      const current = folderData[key];
      if (current?.loaded || current?.loading) return;
      setFolderData((prev) => ({ ...prev, [key]: { items: [], loaded: false, loading: true } }));
      try {
        const items = await spec.load(api, schema);
        setFolderData((prev) => ({ ...prev, [key]: { items, loaded: true, loading: false } }));
      } catch {
        setFolderData((prev) => ({ ...prev, [key]: { items: [], loaded: true, loading: false } }));
      }
    },
    [folderData, api],
  );

  const toggleFolder = useCallback(
    (schema: string, spec: FolderSpec) => {
      const key = `${schema}:${spec.kind}`;
      toggle(key);
      if (!expanded.has(key)) loadFolder(schema, spec);
    },
    [toggle, expanded, loadFolder],
  );

  const toggleExtensions = useCallback(() => {
    toggle("extensions");
    if (extensions === null) api.getExtensions().then(setExtensions).catch(() => setExtensions([]));
  }, [toggle, extensions, api]);

  /** The open page belongs to this item (a table's structure tab counts as the table). */
  function isActive(spec: FolderSpec, item: FolderItem, path: string, schema: string) {
    if (spec.kind === "functions") return location.pathname.includes("/function/") && params.schema === schema && params.name === item.name;
    const current = decodeURIComponent(location.pathname);
    const target = decodeURIComponent(path);
    return current === target || current === `${target}/structure`;
  }

  if (loading) {
    return (
      <div className="px-3 py-4 flex justify-end">
        <Loader size={14} className="animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="px-2 select-none">
      {schemas.map((schema) => {
        const isExpanded = expanded.has(schema.name);
        return (
          <div key={schema.name}>
            <button
              onClick={() => toggle(schema.name)}
              className="w-full flex h-7 items-center gap-1.5 rounded-md px-1.5 hover:bg-accent text-muted-foreground hover:text-foreground transition-colors group"
            >
              <Chevron open={isExpanded} />
              <Folder size={15} className="shrink-0 text-warning-foreground/80" />
              <span className="truncate text-sm">{schema.name}</span>
            </button>

            {isExpanded &&
              FOLDERS.map((spec) => {
                const key = `${schema.name}:${spec.kind}`;
                const folder = folderData[key] ?? EMPTY_FOLDER;
                const Icon = spec.icon;
                return (
                  <SubFolder
                    key={spec.kind}
                    label={spec.label}
                    folderColor={`${spec.color} opacity-70`}
                    open={expanded.has(key)}
                    folder={folder}
                    onToggle={() => toggleFolder(schema.name, spec)}
                  >
                    {folder.items.map((item) => {
                      const view = spec.view(item, schema.name);
                      const active = isActive(spec, item, view.path, schema.name);
                      return (
                        <button
                          key={view.key}
                          onClick={() => navigate(view.path)}
                          className={`w-full flex h-7 items-center gap-2 rounded-md pl-11 pr-2 text-sm transition-colors ${
                            active ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground hover:bg-accent"
                          }`}
                        >
                          <Icon size={13} className={`shrink-0 ${spec.color} ${active ? "" : "opacity-70"}`} />
                          <span className="truncate">{view.label}</span>
                          {view.hint && <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/70">{view.hint}</span>}
                        </button>
                      );
                    })}
                  </SubFolder>
                );
              })}
          </div>
        );
      })}

      {schemas.length === 0 && (
        <div className="px-3 py-6 text-muted-foreground/70 text-center text-sm">
          no schemas found
        </div>
      )}

      {/* Extensions belong to the database, not to one schema */}
      <button
        onClick={toggleExtensions}
        className="w-full flex h-7 items-center gap-1.5 rounded-md px-1.5 hover:bg-accent text-muted-foreground hover:text-foreground transition-colors group"
      >
        <Chevron open={expanded.has("extensions")} />
        <Puzzle size={15} className="shrink-0 text-orange-500/80" />
        <span className="truncate text-sm">Extensions</span>
        {extensions && extensions.length > 0 && (
          <span className="ml-auto text-[10px] text-muted-foreground/70">{extensions.length}</span>
        )}
      </button>
      {expanded.has("extensions") &&
        (extensions === null ? (
          <div className="pl-7 py-1.5">
            <Loader size={10} className="animate-spin text-muted-foreground" />
          </div>
        ) : extensions.length === 0 ? (
          <div className="pl-7 py-1.5 text-xs text-muted-foreground/70">none installed</div>
        ) : (
          extensions.map((ext) => (
            <div key={ext.name} className="flex h-7 items-center gap-2 rounded-md pl-7 pr-2 text-sm text-muted-foreground">
              <Puzzle size={13} className="shrink-0 text-orange-500/70" />
              <span className="truncate">{ext.name}</span>
              <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground/70">{ext.version}</span>
            </div>
          ))
        ))}
    </div>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <ChevronRight
      size={10}
      className={`shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground transition-all duration-150 ${open ? "rotate-90" : ""}`}
    />
  );
}

function SubFolder({
  label,
  folderColor,
  open,
  folder,
  onToggle,
  children,
}: {
  label: string;
  folderColor: string;
  open: boolean;
  folder: FolderData;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div>
      <button
        onClick={onToggle}
        className="w-full flex h-7 items-center gap-1.5 rounded-md pl-5 pr-2 hover:bg-accent text-muted-foreground hover:text-foreground transition-colors group"
      >
        <Chevron open={open} />
        <Folder size={15} className={`shrink-0 ${folderColor}`} />
        <span className="truncate text-sm">{label}</span>
        {folder.loading && (
          <Loader size={10} className="ml-auto shrink-0 animate-spin text-muted-foreground" />
        )}
        {folder.loaded && folder.items.length > 0 && (
          <span className="ml-auto text-[10px] text-muted-foreground/70">
            {folder.items.length}
          </span>
        )}
      </button>
      {open && folder.loaded && (
        <div>
          {folder.items.length === 0 ? (
            <div className="pl-11 py-1.5 text-xs text-muted-foreground/70">empty</div>
          ) : (
            children
          )}
        </div>
      )}
    </div>
  );
}
