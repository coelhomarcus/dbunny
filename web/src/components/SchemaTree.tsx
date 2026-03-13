import { useState, useEffect, useCallback } from "react";
import { useNavigate, useParams } from "react-router";
import { api } from "../lib/api";
import type { SchemaInfo, TableInfo, FunctionInfo } from "@/types";
import {
  ChevronRight,
  Folder,
  Table2,
  Eye,
  Braces,
  Loader,
} from "lucide-react";

interface SchemaData {
  tables: TableInfo[];
  views: TableInfo[];
  functions: FunctionInfo[];
  loaded: boolean;
}

export default function SchemaTree() {
  const [schemas, setSchemas] = useState<SchemaInfo[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [schemaData, setSchemaData] = useState<Record<string, SchemaData>>({});
  const [loading, setLoading] = useState(true);
  const navigate = useNavigate();
  const params = useParams();
  const activeTable = params.table;
  const activeSchema = params.schema;

  useEffect(() => {
    api
      .getSchemas()
      .then(async (s) => {
        setSchemas(s);
        setLoading(false);

      })
      .catch(() => setLoading(false));
  }, []);

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

  const toggleSchema = useCallback(
    async (schema: string) => {
      toggle(schema);

      if (!schemaData[schema]?.loaded) {
        try {
          const [tables, views, functions] = await Promise.all([
            api.getTables(schema),
            api.getViews(schema),
            api.getFunctions(schema),
          ]);
          setSchemaData((prev) => ({
            ...prev,
            [schema]: { tables, views, functions, loaded: true },
          }));
        } catch {
          // ignore
        }
      }
    },
    [schemaData, toggle],
  );

  if (loading) {
    return (
      <div className="px-3 py-4 flex justify-end">
        <Loader size={14} className="animate-spin text-zinc-500" />
      </div>
    );
  }

  return (
    <div className="text-sm select-none">
      {schemas.map((schema) => {
        const data = schemaData[schema.name];
        const isExpanded = expanded.has(schema.name);

        return (
          <div key={schema.name}>
            {/* Schema row */}
            <button
              onClick={() => toggleSchema(schema.name)}
              className="w-full flex items-center gap-1.5 px-1.5 py-1.5 hover:bg-zinc-800/50 text-zinc-400 hover:text-zinc-200 transition-colors group"
            >
              <ChevronRight
                size={10}
                className={`shrink-0 text-zinc-600 group-hover:text-zinc-400 transition-all duration-150 ${isExpanded ? "rotate-90" : ""}`}
              />
              <Folder size={13} className="shrink-0 text-amber-500/80" />
              <span className="truncate text-xs">{schema.name}</span>
              {isExpanded && !data?.loaded && (
                <Loader
                  size={12}
                  className="ml-auto shrink-0 animate-spin text-zinc-500"
                />
              )}
            </button>

            {/* Schema contents */}
            {isExpanded && data?.loaded && (
              <div>
                {/* Tables folder */}
                {data.tables.length > 0 && (
                  <div>
                    <button
                      onClick={() => toggle(`${schema.name}:tables`)}
                      className="w-full flex items-center gap-1.5 pl-5 pr-3 py-1.5 hover:bg-zinc-800/50 text-zinc-400 hover:text-zinc-200 transition-colors group"
                    >
                      <ChevronRight
                        size={10}
                        className={`shrink-0 text-zinc-600 group-hover:text-zinc-400 transition-all duration-150 ${expanded.has(`${schema.name}:tables`) ? "rotate-90" : ""}`}
                      />
                      <Folder size={13} className="shrink-0 text-green-500/60" />
                      <span className="truncate text-xs">Tables</span>
                      <span className="ml-auto text-[10px] text-zinc-600">{data.tables.length}</span>
                    </button>
                    {expanded.has(`${schema.name}:tables`) && (
                      <div>
                        {data.tables.map((t) => {
                          const isActive =
                            activeSchema === schema.name && activeTable === t.name;
                          return (
                            <button
                              key={t.name}
                              onClick={() =>
                                navigate(`/db/${schema.name}/${t.name}`)
                              }
                              className={`w-full flex items-center gap-2 pl-12 pr-3 py-1.5 text-xs transition-colors ${
                                isActive
                                  ? "bg-zinc-800 text-white"
                                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
                              }`}
                            >
                              <Table2
                                size={13}
                                className={`shrink-0 ${isActive ? "text-green-400" : "text-green-500/60"}`}
                              />
                              <span className="truncate">{t.name}</span>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                )}

                {/* Views folder */}
                {data.views.length > 0 && (
                  <div>
                    <button
                      onClick={() => toggle(`${schema.name}:views`)}
                      className="w-full flex items-center gap-1.5 pl-5 pr-3 py-1.5 hover:bg-zinc-800/50 text-zinc-400 hover:text-zinc-200 transition-colors group"
                    >
                      <ChevronRight
                        size={10}
                        className={`shrink-0 text-zinc-600 group-hover:text-zinc-400 transition-all duration-150 ${expanded.has(`${schema.name}:views`) ? "rotate-90" : ""}`}
                      />
                      <Folder size={13} className="shrink-0 text-blue-500/60" />
                      <span className="truncate text-xs">Views</span>
                      <span className="ml-auto text-[10px] text-zinc-600">{data.views.length}</span>
                    </button>
                    {expanded.has(`${schema.name}:views`) && (
                      <div>
                        {data.views.map((v) => {
                          const isActive =
                            activeSchema === schema.name && activeTable === v.name;
                          return (
                            <button
                              key={v.name}
                              onClick={() =>
                                navigate(`/db/${schema.name}/${v.name}`)
                              }
                              className={`w-full flex items-center gap-2 pl-12 pr-3 py-1.5 text-xs transition-colors ${
                                isActive
                                  ? "bg-zinc-800 text-white"
                                  : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
                              }`}
                            >
                              <Eye
                                size={13}
                                className={`shrink-0 ${isActive ? "text-blue-400" : "text-blue-500/60"}`}
                              />
                              <span className="truncate">{v.name}</span>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                )}

                {/* Functions folder */}
                {data.functions.length > 0 && (
                  <div>
                    <button
                      onClick={() => toggle(`${schema.name}:functions`)}
                      className="w-full flex items-center gap-1.5 pl-5 pr-3 py-1.5 hover:bg-zinc-800/50 text-zinc-400 hover:text-zinc-200 transition-colors group"
                    >
                      <ChevronRight
                        size={10}
                        className={`shrink-0 text-zinc-600 group-hover:text-zinc-400 transition-all duration-150 ${expanded.has(`${schema.name}:functions`) ? "rotate-90" : ""}`}
                      />
                      <Folder size={13} className="shrink-0 text-purple-500/60" />
                      <span className="truncate text-xs">Functions</span>
                      <span className="ml-auto text-[10px] text-zinc-600">{data.functions.length}</span>
                    </button>
                    {expanded.has(`${schema.name}:functions`) && (
                      <div>
                        {data.functions.map((f) => (
                          <div
                            key={`${f.name}-${f.argumentTypes}`}
                            className="flex items-center gap-2 pl-12 pr-3 py-1.5 text-xs text-zinc-500"
                          >
                            <Braces
                              size={13}
                              className="shrink-0 text-purple-500/60"
                            />
                            <span className="truncate">{f.name}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {!data.tables.length &&
                  !data.views.length &&
                  !data.functions.length && (
                    <div className="pl-8 py-2 text-[11px] text-zinc-600">
                      empty schema
                    </div>
                  )}
              </div>
            )}
          </div>
        );
      })}

      {schemas.length === 0 && (
        <div className="px-3 py-6 text-zinc-600 text-center text-xs">
          no schemas found
        </div>
      )}
    </div>
  );
}
