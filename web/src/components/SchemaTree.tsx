import { useState, useEffect, useCallback } from "react";
import { useNavigate, useParams } from "react-router";
import { useApi } from "../contexts/useConnection";
import type { SchemaInfo, TableInfo, FunctionInfo } from "@/types";
import {
  ChevronRight,
  Folder,
  Table2,
  Eye,
  Braces,
  Loader,
} from "lucide-react";

type FolderKind = "tables" | "views" | "functions";

interface FolderData<T> {
  items: T[];
  loaded: boolean;
  loading: boolean;
}

interface SchemaFolders {
  tables: FolderData<TableInfo>;
  views: FolderData<TableInfo>;
  functions: FolderData<FunctionInfo>;
}

const emptyFolders: SchemaFolders = {
  tables: { items: [], loaded: false, loading: false },
  views: { items: [], loaded: false, loading: false },
  functions: { items: [], loaded: false, loading: false },
};

interface SchemaTreeProps {
  schemaToExpand?: string | null;
  onSchemaExpanded?: () => void;
}

export default function SchemaTree({ schemaToExpand, onSchemaExpanded }: SchemaTreeProps = {}) {
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [folderData, setFolderData] = useState<Record<string, SchemaFolders>>({});
  const [loading, setLoading] = useState(true);
  const navigate = useNavigate();
  const params = useParams();
  const api = useApi();
  const activeTable = params.table;
  const activeSchema = params.schema;
  const activeFunction = params.name;

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
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const loadFolder = useCallback(
    async (schema: string, kind: FolderKind) => {
      const current = folderData[schema]?.[kind];
      if (current?.loaded || current?.loading) return;

      setFolderData((prev) => ({
        ...prev,
        [schema]: {
          ...(prev[schema] ?? emptyFolders),
          [kind]: { items: [], loaded: false, loading: true },
        },
      }));

      try {
        let items: TableInfo[] | FunctionInfo[];
        if (kind === "tables") items = await api.getTables(schema);
        else if (kind === "views") items = await api.getViews(schema);
        else items = await api.getFunctions(schema);

        setFolderData((prev) => ({
          ...prev,
          [schema]: {
            ...(prev[schema] ?? emptyFolders),
            [kind]: { items, loaded: true, loading: false },
          },
        }));
      } catch {
        setFolderData((prev) => ({
          ...prev,
          [schema]: {
            ...(prev[schema] ?? emptyFolders),
            [kind]: { items: [], loaded: true, loading: false },
          },
        }));
      }
    },
    [folderData, api],
  );

  const toggleFolder = useCallback(
    (schema: string, kind: FolderKind) => {
      const key = `${schema}:${kind}`;
      toggle(key);
      if (!expanded.has(key)) {
        loadFolder(schema, kind);
      }
    },
    [toggle, expanded, loadFolder],
  );

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
        const data = folderData[schema.name] ?? emptyFolders;

        return (
          <div key={schema.name}>
            <button
              onClick={() => toggle(schema.name)}
              className="w-full flex h-7 items-center gap-1.5 rounded-md px-1.5 hover:bg-accent text-muted-foreground hover:text-foreground transition-colors group"
            >
              <ChevronRight
                size={10}
                className={`shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground transition-all duration-150 ${isExpanded ? "rotate-90" : ""}`}
              />
              <Folder size={15} className="shrink-0 text-warning-foreground/80" />
              <span className="truncate text-sm">{schema.name}</span>
            </button>

            {isExpanded && (
              <div>
                <SubFolder
                  schemaName={schema.name}
                  kind="tables"
                  label="Tables"
                  folderColor="text-success-foreground/70"
                  expanded={expanded}
                  folder={data.tables}
                  onToggle={toggleFolder}
                >
                  {(data.tables.items as TableInfo[]).map((t) => {
                    const isActive =
                      activeSchema === schema.name && activeTable === t.name;
                    return (
                      <button
                        key={t.name}
                        onClick={() => navigate(`/db/${schema.name}/${t.name}`)}
                        className={`w-full flex h-7 items-center gap-2 rounded-md pl-11 pr-2 text-sm transition-colors ${
                          isActive
                            ? "bg-accent text-foreground"
                            : "text-muted-foreground hover:text-foreground hover:bg-accent"
                        }`}
                      >
                        <Table2
                          size={13}
                          className={`shrink-0 ${isActive ? "text-success-foreground" : "text-success-foreground/70"}`}
                        />
                        <span className="truncate">{t.name}</span>
                      </button>
                    );
                  })}
                </SubFolder>

                <SubFolder
                  schemaName={schema.name}
                  kind="views"
                  label="Views"
                  folderColor="text-info-foreground/70"
                  expanded={expanded}
                  folder={data.views}
                  onToggle={toggleFolder}
                >
                  {(data.views.items as TableInfo[]).map((v) => {
                    const isActive =
                      activeSchema === schema.name && activeTable === v.name;
                    return (
                      <button
                        key={v.name}
                        onClick={() => navigate(`/db/${schema.name}/${v.name}`)}
                        className={`w-full flex h-7 items-center gap-2 rounded-md pl-11 pr-2 text-sm transition-colors ${
                          isActive
                            ? "bg-accent text-foreground"
                            : "text-muted-foreground hover:text-foreground hover:bg-accent"
                        }`}
                      >
                        <Eye
                          size={13}
                          className={`shrink-0 ${isActive ? "text-info-foreground" : "text-info-foreground/70"}`}
                        />
                        <span className="truncate">{v.name}</span>
                      </button>
                    );
                  })}
                </SubFolder>

                <SubFolder
                  schemaName={schema.name}
                  kind="functions"
                  label="Functions"
                  folderColor="text-purple-500/70"
                  expanded={expanded}
                  folder={data.functions}
                  onToggle={toggleFolder}
                >
                  {(data.functions.items as FunctionInfo[]).map((f) => {
                    const isActive =
                      activeSchema === schema.name && activeFunction === f.name;
                    return (
                      <button
                        key={`${f.name}-${f.argumentTypes}`}
                        onClick={() =>
                          navigate(
                            `/db/${schema.name}/function/${f.name}?args=${encodeURIComponent(f.argumentTypes)}`
                          )
                        }
                        className={`w-full flex h-7 items-center gap-2 rounded-md pl-11 pr-2 text-sm transition-colors ${
                          isActive
                            ? "bg-accent text-foreground"
                            : "text-muted-foreground hover:text-foreground hover:bg-accent"
                        }`}
                      >
                        <Braces
                          size={13}
                          className={`shrink-0 ${isActive ? "text-purple-500" : "text-purple-500/70"}`}
                        />
                        <span className="truncate">{f.name}</span>
                      </button>
                    );
                  })}
                </SubFolder>
              </div>
            )}
          </div>
        );
      })}

      {schemas.length === 0 && (
        <div className="px-3 py-6 text-muted-foreground/70 text-center text-sm">
          no schemas found
        </div>
      )}
    </div>
  );
}

function SubFolder({
  schemaName,
  kind,
  label,
  folderColor,
  expanded,
  folder,
  onToggle,
  children,
}: {
  schemaName: string;
  kind: FolderKind;
  label: string;
  folderColor: string;
  expanded: Set<string>;
  folder: FolderData<unknown>;
  onToggle: (schema: string, kind: FolderKind) => void;
  children: React.ReactNode;
}) {
  const key = `${schemaName}:${kind}`;
  const isOpen = expanded.has(key);

  return (
    <div>
      <button
        onClick={() => onToggle(schemaName, kind)}
        className="w-full flex h-7 items-center gap-1.5 rounded-md pl-5 pr-2 hover:bg-accent text-muted-foreground hover:text-foreground transition-colors group"
      >
        <ChevronRight
          size={10}
          className={`shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground transition-all duration-150 ${isOpen ? "rotate-90" : ""}`}
        />
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
      {isOpen && folder.loaded && (
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
