import { useState, useEffect, useCallback, useMemo, type ComponentType, type ReactNode } from "react";
import { useDocuments } from "../contexts/DocumentsContext";
import { useNotify } from "../contexts/NoticeProvider";
import { useTab } from "../contexts/workspace";
import type { DataIcon } from "../lib/documents";
import { bindSchemaRefresh } from "../lib/schemaRefresh";
import { useApi, useConnection } from "../contexts/useConnection";
import type { RelationKind } from "../lib/api";
import ContextMenu, { type ContextMenuItem } from "./ui/ContextMenu";
import RelationActionDialog, { type RelationAction, type RelationTarget } from "./schema/RelationActionDialog";
import { writeClipboardText } from "../lib/clipboard";
import ContainerDialog, { type ContainerOptions, type ContainerRequest } from "./schema/ContainerDialog";
import { quoteIdentFor } from "../lib/copyRows";
import { shortType } from "../lib/columnMeta";
import { cn } from "../lib/cn";
import type { Capabilities, ColumnInfo, ExplorerObject, ExplorerPartition, ExplorerTree, ExtensionInfo } from "@/types";
import { useCapabilities } from "../hooks/useCapabilities";
import { useSqlDialect } from "../hooks/useSqlDialect";
import Button from "./ui/Button";
import Tooltip from "./Tooltip";
import {
  ChevronRight,
  Folder,
  Table2,
  Eye,
  Layers,
  Braces,
  ListTree,
  Zap,
  ListOrdered,
  Shapes,
  Puzzle,
  Copy,
  PenLine,
  Trash2,
  Eraser,
  RefreshCw,
  Table as TableIcon,
  Columns3,
  CopyPlus,
  Plus,
  Search,
  X,
  Pin,
  PinOff,
  ChevronsDownUp,
  ChevronsUpDown,
  KeyRound,
  Link2,
  Database,
  FolderPlus,
  Split,
  Waypoints,
  type LucideProps,
} from "lucide-react";
import { useT } from "../i18n";

type ObjectKind = ExplorerObject["kind"];

/** How each kind of object is grouped, drawn and opened. */
interface Category {
  id: string;
  label: string;
  kinds: ObjectKind[];
  icon: ComponentType<LucideProps>;
  /** Text colour class for the icons. */
  color: string;
  /** Open when the explorer first shows a schema. */
  openByDefault?: boolean;
}

const CATEGORIES: Category[] = [
  { id: "tables", label: "Tables", kinds: ["table"], icon: Table2, color: "text-success-foreground", openByDefault: true },
  { id: "views", label: "Views", kinds: ["view"], icon: Eye, color: "text-info-foreground", openByDefault: true },
  { id: "matviews", label: "Materialized views", kinds: ["matview"], icon: Layers, color: "text-cyan-500" },
  { id: "functions", label: "Functions", kinds: ["function", "procedure"], icon: Braces, color: "text-purple-500" },
  { id: "sequences", label: "Sequences", kinds: ["sequence"], icon: ListOrdered, color: "text-teal-500" },
  { id: "types", label: "Types", kinds: ["type"], icon: Shapes, color: "text-pink-500" },
  { id: "indexes", label: "Indexes", kinds: ["index"], icon: ListTree, color: "text-orange-500" },
  { id: "triggers", label: "Triggers", kinds: ["trigger"], icon: Zap, color: "text-yellow-500" },
];

/** Categories shown only when the engine has that kind of object. */
const CATEGORY_CAPABILITY: Partial<Record<string, keyof Capabilities>> = {
  matviews: "materializedViews",
  functions: "functions",
  sequences: "sequences",
  types: "types",
};

const RELATION_KINDS: Partial<Record<ObjectKind, RelationKind>> = { table: "table", view: "view", matview: "matview" };

const objectKey = (schema: string, object: ExplorerObject) => `${schema}.${object.name}:${object.kind}:${object.detail}`;

/** A table pinned to the top of the explorer. */
interface PinnedObject {
  schema: string;
  name: string;
  kind: ObjectKind;
}

function loadPins(key: string | null): PinnedObject[] {
  if (!key) return [];
  try {
    const parsed = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

/** The name with the filter's match underlined. */
function Highlight({ text, query }: { text: string; query: string }) {
  const at = query ? text.toLowerCase().indexOf(query.toLowerCase()) : -1;
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <span className="rounded-sm bg-primary/20 text-foreground">{text.slice(at, at + query.length)}</span>
      {text.slice(at + query.length)}
    </>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <ChevronRight
      size={10}
      className={`shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground transition-transform duration-150 ${open ? "rotate-90" : ""}`}
    />
  );
}

/** Indentation steps, so nested rows line up whatever the depth. */
const INDENT = ["pl-1.5", "pl-5", "pl-9", "pl-[3.25rem]", "pl-[4.25rem]"];

function GroupRow({
  depth,
  open,
  onToggle,
  icon,
  label,
  action,
  onContextMenu,
}: {
  depth: number;
  open: boolean;
  onToggle: () => void;
  icon: ReactNode;
  label: string;
  action?: ReactNode;
  onContextMenu?: (e: React.MouseEvent) => void;
}) {
  return (
    <div className="group relative" onContextMenu={onContextMenu}>
      <button
        onClick={onToggle}
        aria-expanded={open}
        className={cn(
          "flex h-7 w-full items-center gap-1.5 rounded-md text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
          INDENT[depth],
          action ? "pr-8" : "pr-2",
        )}
      >
        <Chevron open={open} />
        {icon}
        <span className="min-w-0 truncate">{label}</span>
      </button>
      {action && <span className="absolute right-1.5 top-1/2 flex -translate-y-1/2">{action}</span>}
    </div>
  );
}

/**
 * The database's objects: filterable, with pinned tables on top, one row per schema,
 * categories that only show when they have something, and columns under each table.
 */
export default function SchemaTree() {
  const t = useT();
  const { documents, openDocument, renameRelation, renameSchema, dropRelation, dropSchema, refreshData } = useDocuments();
  const { tabId } = useTab();
  const api = useApi();
  const { connectionInfo } = useConnection();
  const readOnly = connectionInfo?.readOnly ?? false;
  const capabilities = useCapabilities();
  const quoteIdent = quoteIdentFor(useSqlDialect());
  const pinsKey = connectionInfo ? `dbunny:pins:${connectionInfo.host}:${connectionInfo.port}/${connectionInfo.database}` : null;

  const [tree, setTree] = useState<ExplorerTree | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [pins, setPins] = useState<PinnedObject[]>(() => loadPins(pinsKey));
  const [columns, setColumns] = useState<Record<string, ColumnInfo[] | "loading" | "error">>({});
  const [extensions, setExtensions] = useState<ExtensionInfo[] | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; target: RelationTarget } | null>(null);
  const [plainMenu, setPlainMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  // Right click on a schema, or the header's "+" (schema null)
  const [schemaMenu, setSchemaMenu] = useState<{ x: number; y: number; schema: string | null } | null>(null);
  const [container, setContainer] = useState<ContainerRequest | null>(null);
  const [action, setAction] = useState<{ action: RelationAction; target: RelationTarget } | null>(null);
  const notify = useNotify();

  const load = useCallback(
    async (first: boolean) => {
      setRefreshing(true);
      try {
        const next = await api.getExplorer();
        setTree(next);
        setError(null);
        // Columns may have changed too; they load again when opened
        setColumns({});
        if (first) {
          // The default schema starts open, with its tables and views
          const home = next.schemas.find((s) => s.name === next.defaultSchema) ?? next.schemas.find((s) => s.objects.length > 0);
          const open = new Set<string>();
          if (home) {
            open.add(`s:${home.name}`);
            for (const c of CATEGORIES) if (c.openByDefault) open.add(`c:${home.name}:${c.id}`);
          }
          setExpanded(open);
        }
      } catch (err) {
        const text = typeof err === "string" ? err : "Couldn't read the database's objects";
        notify({ tone: "error", text });
        setError(text);
      } finally {
        setRefreshing(false);
      }
    },
    [api, notify],
  );

  useEffect(() => {
    // Deferred so the effect doesn't set state synchronously
    const timer = setTimeout(() => load(true), 0);
    return () => clearTimeout(timer);
  }, [load]);

  useEffect(
    () =>
      bindSchemaRefresh(tabId, (schema) => {
        void load(false);
        setExpanded((prev) => new Set([...prev, `s:${schema}`, `c:${schema}:tables`]));
      }),
    [tabId, load],
  );


  const toggle = useCallback((key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  function toggleColumns(schema: string, name: string) {
    const key = `t:${schema}.${name}`;
    toggle(key);
    if (expanded.has(key) || columns[key] === "loading" || Array.isArray(columns[key])) return;
    setColumns((prev) => ({ ...prev, [key]: "loading" }));
    api
      .getColumns(schema, name)
      .then((cols) => setColumns((prev) => ({ ...prev, [key]: cols })))
      .catch(() => {
        setColumns((prev) => ({ ...prev, [key]: "error" }));
        notify({ tone: "error", text: `Couldn't read the columns of ${name}` });
      });
  }

  function togglePin(schema: string, object: ExplorerObject) {
    const pinned = pins.some((p) => p.schema === schema && p.name === object.name && p.kind === object.kind);
    const next = pinned
      ? pins.filter((p) => !(p.schema === schema && p.name === object.name && p.kind === object.kind))
      : [...pins, { schema, name: object.name, kind: object.kind }];
    setPins(next);
    if (!pinsKey) return;
    try {
      localStorage.setItem(pinsKey, JSON.stringify(next));
    } catch {
      // pinned for this session only
    }
  }

  const query = filter.trim();
  const matches = useCallback((name: string) => !query || name.toLowerCase().includes(query.toLowerCase()), [query]);
  const total = useMemo(() => tree?.schemas.reduce((n, s) => n + s.objects.length, 0) ?? 0, [tree]);
  const shown = useMemo(
    () => tree?.schemas.reduce((n, s) => n + s.objects.filter((o) => matches(o.name)).length, 0) ?? 0,
    [tree, matches],
  );
  const multiSchema = (tree?.schemas.length ?? 0) > 1;
  // While filtering everything with a match is open, whatever was expanded before
  const isOpen = (key: string) => (query ? !key.startsWith("t:") || expanded.has(key) : expanded.has(key));

  /** The engine has this category of objects. */
  const categoryShown = (category: Category) => {
    const capability = CATEGORY_CAPABILITY[category.id];
    return !capability || capabilities[capability] === true;
  };

  /** The front document is this object. A structure tab counts as its table. */
  function isActive(schema: string, object: ExplorerObject) {
    const front = documents.items.find((doc) => doc.id === documents.activeId);
    if (!front || front.schema !== schema || front.name !== object.name) return false;
    if (object.kind === "table" || object.kind === "view" || object.kind === "matview") {
      return front.kind === "data" || front.kind === "structure";
    }
    if (object.kind === "function" || object.kind === "procedure") {
      return front.kind === object.kind && (front.detail ?? "") === (object.detail ?? "");
    }
    return front.kind === object.kind;
  }

  function openExplorerObject(schema: string, object: ExplorerObject) {
    if (object.kind === "table" || object.kind === "view" || object.kind === "matview") {
      const icon: DataIcon = object.kind === "view" ? "view" : object.kind === "matview" ? "matview" : "table";
      openDocument({ kind: "data", schema, name: object.name, icon });
      return;
    }
    if (object.kind === "function" || object.kind === "procedure") {
      openDocument({ kind: object.kind, schema, name: object.name, detail: object.detail });
      return;
    }
    openDocument({ kind: object.kind, schema, name: object.name });
  }

  async function runAction(
    { action: kind, target }: { action: RelationAction; target: RelationTarget },
    options: { newName: string; withData: boolean; cascade: boolean; restartIdentity: boolean },
  ) {
    if (kind === "rename") {
      await api.renameRelation(target.schema, target.name, target.kind, options.newName);
      renameRelation(target.schema, target.name, options.newName);
    } else if (kind === "duplicate") {
      await api.duplicateTable(target.schema, target.name, options.newName, options.withData);
    } else if (kind === "truncate") {
      await api.truncateTable(target.schema, target.name, options.restartIdentity, options.cascade);
      refreshData(target.schema, target.name);
    } else {
      await api.dropRelation(target.schema, target.name, target.kind, options.cascade);
      dropRelation(target.schema, target.name);
    }
    load(false);
  }

  async function runContainer(request: ContainerRequest, options: ContainerOptions) {
    const { action: kind, name } = request;
    if (kind === "create") {
      await api.createSchema(options.name);
      setExpanded((prev) => new Set([...prev, `s:${options.name}`]));
    } else if (kind === "rename") {
      await api.renameSchema(name, options.name);
      setPins((prev) => prev.map((p) => (p.schema === name ? { ...p, schema: options.name } : p)));
      renameSchema(name, options.name);
    } else {
      await api.dropSchema(name, options.cascade);
      setPins((prev) => prev.filter((p) => p.schema !== name));
      dropSchema(name);
    }
    load(false);
  }

  function schemaMenuItems(schema: string | null): ContextMenuItem[] {
    const diagramOf = (name: string): ContextMenuItem => ({
      label: "Diagram",
      icon: <Waypoints />,
      onSelect: () => openDocument({ kind: "diagram", schema: name }),
    });
    const newSchema: ContextMenuItem = {
      label: "New schema…",
      icon: <FolderPlus />,
      onSelect: () => setContainer({ kind: "schema", action: "create", name: "" }),
    };
    // The header's "+" never opens a diagram: that stays on the schema, one schema or many.
    if (schema === null) {
      if (readOnly) return [];
      if (multiSchema) return capabilities.schemas ? [newSchema] : [];
      return [
        { label: "New table", icon: <Table2 />, onSelect: () => openDocument({ kind: "create-table", schema: defaultSchema }) },
        ...(capabilities.schemas ? [newSchema] : []),
      ];
    }
    const items: ContextMenuItem[] = [
      diagramOf(schema),
      { label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(quoteIdent(schema)) },
    ];
    if (readOnly) return items;
    return [
      diagramOf(schema),
      { label: "New table here", icon: <Table2 />, onSelect: () => openDocument({ kind: "create-table", schema }) },
      ...(capabilities.schemas ? [newSchema] : []),
      ...items.slice(1),
      ...(capabilities.schemas
        ? ([
            "separator",
            { label: "Rename…", icon: <PenLine />, onSelect: () => setContainer({ kind: "schema", action: "rename", name: schema }) },
            { label: "Drop…", icon: <Trash2 />, destructive: true, onSelect: () => setContainer({ kind: "schema", action: "drop", name: schema }) },
          ] as ContextMenuItem[])
        : []),
    ];
  }

  function menuItems(target: RelationTarget): ContextMenuItem[] {
    const qualified = `${quoteIdent(target.schema)}.${quoteIdent(target.name)}`;
    const object = { name: target.name, kind: target.kind as ObjectKind, detail: "" };
    const pinned = pins.some((p) => p.schema === target.schema && p.name === target.name && p.kind === target.kind);
    const items: ContextMenuItem[] = [
      { label: "Open data", icon: <TableIcon />, onSelect: () => openDocument({ kind: "data", schema: target.schema, name: target.name, icon: target.kind === "view" ? "view" : target.kind === "matview" ? "matview" : "table" }) },
      { label: "Open structure", icon: <Columns3 />, onSelect: () => openDocument({ kind: "structure", schema: target.schema, name: target.name }) },
      { label: pinned ? "Unpin" : "Pin to the top", icon: pinned ? <PinOff /> : <Pin />, onSelect: () => togglePin(target.schema, object) },
      "separator",
      { label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(qualified) },
      { label: "Copy SELECT", icon: <Copy />, onSelect: () => writeClipboardText(`SELECT * FROM ${qualified} LIMIT 100;`) },
    ];
    if (readOnly) return items;
    items.push("separator", { label: "Rename…", icon: <PenLine />, onSelect: () => setAction({ action: "rename", target }) });
    if (target.kind === "table" && capabilities.duplicateTable) {
      items.push({ label: "Duplicate…", icon: <CopyPlus />, onSelect: () => setAction({ action: "duplicate", target }) });
    }
    if (target.kind === "matview" && capabilities.materializedViews) {
      items.push({
        label: "Refresh data",
        icon: <RefreshCw />,
        onSelect: () => {
          api
            .refreshMaterializedView(target.schema, target.name)
            .then(() => refreshData(target.schema, target.name))
            .catch((err) => notify({ tone: "error", text: typeof err === "string" ? err : "Refresh failed" }));
        },
      });
    }
    items.push("separator");
    if (target.kind === "table") {
      items.push({ label: "Truncate…", icon: <Eraser />, destructive: true, onSelect: () => setAction({ action: "truncate", target }) });
    }
    items.push({ label: "Drop…", icon: <Trash2 />, destructive: true, onSelect: () => setAction({ action: "drop", target }) });
    return items;
  }

  function toggleExtensions() {
    toggle("extensions");
    if (extensions === null) api.getExtensions().then(setExtensions).catch(() => setExtensions([]));
  }

  /** A table's columns, under it when it's expanded. */
  function columnRows(schema: string, name: string, depth: number) {
    const value = columns[`t:${schema}.${name}`];
    const pad = INDENT[Math.min(depth, INDENT.length - 1)];
    if (value === undefined || value === "loading") return <div className={cn("py-1 text-xs text-muted-foreground/70", pad)}>{t("ui.schemaTree.loadingColumns")}</div>;
    if (value === "error") return <div className={cn("py-1 text-xs text-muted-foreground/70", pad)}>{t("ui.schemaTree.columnsFailed")}</div>;
    if (value.length === 0) return <div className={cn("py-1 text-xs text-muted-foreground/70", pad)}>{t("ui.schemaTree.noColumns")}</div>;
    return value.map((col) => (
      <div
        key={col.name}
        onContextMenu={(e) => {
          e.preventDefault();
          setPlainMenu({
            x: e.clientX,
            y: e.clientY,
            items: [{ label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(quoteIdent(col.name)) }],
          });
        }}
        className={cn("flex h-6 items-center gap-1.5 pr-2 text-xs text-muted-foreground", pad)}
      >
        {col.isPrimaryKey ? (
          <KeyRound size={11} className="shrink-0 text-warning-foreground" aria-label={t("ui.schemaTree.primaryKey")} />
        ) : col.foreignKey ? (
          <Link2 size={11} className="shrink-0 text-info-foreground" aria-label={t("ui.schemaTree.foreignKey")} />
        ) : (
          <span className="size-2.75 shrink-0" />
        )}
        <span className="truncate text-foreground/90">{col.name}</span>
        <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground/70">{shortType(col.typeName)}</span>
      </div>
    ));
  }

  function objectRow(schema: string, object: ExplorerObject, depth: number, category: Category, showSchema = false) {
    const relation = RELATION_KINDS[object.kind];
    const active = isActive(schema, object);
    const Icon = category.icon;
    const columnsKey = `t:${schema}.${object.name}`;
    const columnsOpen = relation !== undefined && expanded.has(columnsKey);
    const pinned = pins.some((p) => p.schema === schema && p.name === object.name && p.kind === object.kind);
    const hint =
      object.kind === "type"
        ? object.detail
        : object.kind === "index" || object.kind === "trigger"
          ? `on ${object.detail}`
          : object.kind === "procedure"
            ? "procedure"
            : capabilities.partitions && object.partitions?.length
              ? `${object.partitions.length} partition${object.partitions.length > 1 ? "s" : ""}`
              : object.detail === "foreign"
                ? "foreign"
                : null;
    return (
      <div key={objectKey(schema, object)}>
        <div className="group relative">
          {/* A div, so the columns toggle inside it can be a real button */}
          <div
            role="button"
            tabIndex={0}
            onClick={() => openExplorerObject(schema, object)}
            onKeyDown={(e) => {
              if (e.key === "Enter") openExplorerObject(schema, object);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              if (relation) {
                setMenu({ x: e.clientX, y: e.clientY, target: { schema, name: object.name, kind: relation } });
                return;
              }
              setPlainMenu({
                x: e.clientX,
                y: e.clientY,
                items: [
                  { label: "Open", onSelect: () => openExplorerObject(schema, object) },
                  { label: "Copy name", icon: <Copy />, onSelect: () => writeClipboardText(quoteIdent(object.name)) },
                ],
              });
            }}
            className={cn(
              "flex h-7 w-full items-center gap-1.5 rounded-md pr-8 text-sm transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
              INDENT[Math.min(depth, INDENT.length - 1)],
              active ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
            )}
          >
            {relation ? (
              // The chevron opens the columns; the rest of the row opens the table
              <button
                type="button"
                tabIndex={-1}
                aria-label={t(columnsOpen ? "ui.schemaTree.hideColumnsOf" : "ui.schemaTree.showColumnsOf", { name: object.name })}
                aria-expanded={columnsOpen}
                onClick={(e) => {
                  e.stopPropagation();
                  toggleColumns(schema, object.name);
                }}
                className="-ml-0.5 flex size-4 shrink-0 items-center justify-center rounded cursor-pointer hover:bg-foreground/10"
              >
                <Chevron open={columnsOpen} />
              </button>
            ) : (
              <span className="size-4 shrink-0" />
            )}
            <Icon size={13} className={cn("shrink-0", category.color, !active && "opacity-70")} />
            <span className="truncate">
              {showSchema && <span className="text-muted-foreground/70">{schema}.</span>}
              <Highlight text={object.name} query={query} />
            </span>
            {hint && <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/70">{hint}</span>}
          </div>
          {relation && (
            <button
              type="button"
              aria-label={t(pinned ? "ui.schemaTree.unpin" : "ui.schemaTree.pin", { name: object.name })}
              onClick={() => togglePin(schema, object)}
              className={cn(
                "absolute right-1.5 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground transition-opacity cursor-pointer hover:bg-foreground/10 hover:text-foreground focus-visible:opacity-100",
                pinned ? "opacity-100 text-primary hover:text-primary" : "opacity-0 group-hover:opacity-100",
              )}
            >
              <Pin size={11} className={pinned ? "fill-current" : undefined} />
            </button>
          )}
        </div>
        {columnsOpen && capabilities.partitions && object.partitions?.length ? partitionRows(object.partitions, depth + 2) : null}
        {columnsOpen && columnRows(schema, object.name, depth + 2)}
      </div>
    );
  }

  /** A partitioned table's partitions, above its columns; each one opens like a table. */
  function partitionRows(partitions: ExplorerPartition[], depth: number): ReactNode {
    const pad = INDENT[Math.min(depth, INDENT.length - 1)];
    return partitions.map((p) => {
      const front = documents.items.find((doc) => doc.id === documents.activeId);
      const active = front?.kind === "data" && front.schema === p.schema && front.name === p.name;
      // "FOR VALUES FROM (…) TO (…)" reads fine without its first two words
      const bound = p.bound.replace(/^FOR VALUES\s+/i, "");
      return (
        <div key={`${p.schema}.${p.name}`}>
          <div
            role="button"
            tabIndex={0}
            onClick={() => openDocument({ kind: "data", schema: p.schema, name: p.name, icon: "partition" })}
            onKeyDown={(e) => {
              if (e.key === "Enter") openDocument({ kind: "data", schema: p.schema, name: p.name, icon: "partition" });
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, target: { schema: p.schema, name: p.name, kind: "table" } });
            }}
            className={cn(
              "flex h-6 items-center gap-1.5 rounded-md pr-2 text-xs transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
              pad,
              active ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground",
            )}
          >
            <Split size={11} className="shrink-0 text-success-foreground/70" />
            {/* The name comes first; the bound gets what's left */}
            <span className="max-w-[70%] shrink-0 truncate text-foreground/90">{p.name}</span>
            <span className="ml-auto min-w-0 truncate pl-1 font-mono text-[10px] text-muted-foreground/70">{bound}</span>
          </div>
          {p.partitions?.length ? partitionRows(p.partitions, depth + 1) : null}
        </div>
      );
    });
  }

  function newTableButton(schema: string) {
    if (readOnly) return undefined;
    return (
      <button
        type="button"
        aria-label={t("ui.schemaTree.newTableIn", { schema })}
        onClick={() => openDocument({ kind: "create-table", schema })}
        className="flex size-5 items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity hover:bg-foreground/10 hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100 cursor-pointer"
      >
        <Plus size={12} />
      </button>
    );
  }

  /** The categories of a schema that have something (that matches the filter). */
  function categoryGroups(schema: string, objects: ExplorerObject[], depth: number) {
    return CATEGORIES.map((category) => {
      if (!categoryShown(category)) return null;
      const items = objects.filter((o) => category.kinds.includes(o.kind) && matches(o.name));
      if (items.length === 0) return null;
      const key = `c:${schema}:${category.id}`;
      const Icon = category.icon;
      return (
        <div key={category.id}>
          <GroupRow
            depth={depth}
            open={isOpen(key)}
            onToggle={() => toggle(key)}
            icon={<Icon size={14} className={cn("shrink-0 opacity-70", category.color)} />}
            label={category.label}
            action={category.id === "tables" ? newTableButton(schema) : undefined}
          />
          {isOpen(key) && items.map((object) => objectRow(schema, object, depth + 1, category))}
        </div>
      );
    });
  }

  const pinnedObjects = pins
    .map((pin) => {
      const schema = tree?.schemas.find((s) => s.name === pin.schema);
      const object = schema?.objects.find((o) => o.name === pin.name && o.kind === pin.kind);
      return object ? { schema: pin.schema, object } : null;
    })
    .filter(
      (p): p is { schema: string; object: ExplorerObject } =>
        p !== null && matches(p.object.name) && CATEGORIES.some((c) => c.kinds.includes(p.object.kind) && categoryShown(c)),
    );
  const defaultSchema = tree?.defaultSchema ?? tree?.schemas[0]?.name ?? "public";
  const headerMenuLabel = multiSchema ? "New schema" : capabilities.schemas ? "New table or schema" : "New table";
  // Pinned uses its own flag; every other key is an open group.
  const allClosed = ![...expanded].some((key) => key !== "pinned-closed");

  function toggleAll() {
    if (!tree) return;
    if (!allClosed) {
      setExpanded(new Set());
      return;
    }
    const open = new Set<string>();
    for (const schema of tree.schemas) {
      open.add(`s:${schema.name}`);
      for (const category of CATEGORIES) {
        if (!categoryShown(category)) continue;
        if (schema.objects.some((object) => category.kinds.includes(object.kind) && matches(object.name))) {
          open.add(`c:${schema.name}:${category.id}`);
        }
      }
    }
    if (!query && capabilities.extensions) {
      open.add("extensions");
      if (extensions === null) api.getExtensions().then(setExtensions).catch(() => setExtensions([]));
    }
    setExpanded(open);
  }

  return (
    <div className="flex h-full flex-col select-none">
      {/* Header: what's here, and the explorer's own actions */}
      <div className="flex h-8 shrink-0 items-center gap-1 pl-3 pr-1.5">
        <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/80">{t("ui.schemaTree.explorer")}</span>
        <div className="ml-auto flex items-center">
          {tree && schemaMenuItems(null).length > 0 && (
            <Tooltip label={headerMenuLabel} side="bottom">
              <Button
                variant="ghost-muted"
                size="icon-xs"
                aria-label={headerMenuLabel}
                aria-expanded={schemaMenu?.schema === null}
                onClick={(e) => {
                  // With one thing to make there is nothing to pick from: the button does it (its tooltip says what)
                  const items = schemaMenuItems(null);
                  const only = items.length === 1 ? items[0] : null;
                  if (only && typeof only === "object" && "onSelect" in only && only.onSelect) {
                    only.onSelect();
                    return;
                  }
                  const r = e.currentTarget.getBoundingClientRect();
                  setSchemaMenu({ x: r.left, y: r.bottom + 4, schema: null });
                }}
              >
                <Plus />
              </Button>
            </Tooltip>
          )}
          <Tooltip label={t(allClosed ? "ui.schemaTree.expandAll" : "ui.schemaTree.collapseAll")} side="bottom">
            <Button
              variant="ghost-muted"
              size="icon-xs"
              aria-label={t(allClosed ? "ui.schemaTree.expandAll" : "ui.schemaTree.collapseAll")}
              onClick={toggleAll}
              disabled={!tree}
            >
              {allClosed ? <ChevronsUpDown /> : <ChevronsDownUp />}
            </Button>
          </Tooltip>
          <Tooltip label={t("ui.schemaTree.refresh")} side="bottom">
            <Button variant="ghost-muted" size="icon-xs" aria-label={t("ui.schemaTree.refreshExplorer")} onClick={() => load(false)} disabled={refreshing}>
              <RefreshCw className={refreshing ? "animate-spin" : undefined} />
            </Button>
          </Tooltip>
        </div>
      </div>

      {/* Filter */}
      <div className="relative shrink-0 px-2 pb-1.5">
        <Search size={13} className="pointer-events-none absolute left-4 top-1/2 -translate-y-[calc(50%+3px)] text-muted-foreground" />
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setFilter("");
          }}
          placeholder={t("ui.schemaTree.filterObjects")}
          aria-label={t("ui.schemaTree.filterObjects")}
          spellCheck={false}
          className="h-7 w-full rounded-md border border-transparent bg-muted pl-7 pr-7 text-[13px] text-foreground outline-none placeholder:text-muted-foreground/70 focus:border-ring focus:bg-card"
        />
        {filter && (
          <button
            type="button"
            aria-label={t("ui.schemaTree.clearTheFilter")}
            onClick={() => setFilter("")}
            className="absolute right-3.5 top-1/2 flex size-4 -translate-y-[calc(50%+3px)] items-center justify-center rounded text-muted-foreground hover:text-foreground cursor-pointer"
          >
            <X size={12} />
          </button>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {error ? (
          <div className="flex flex-col items-center gap-2 px-3 py-8 text-center">
            <p className="text-xs text-muted-foreground">{t("ui.schemaTree.objectsFailed")}</p>
            <Button variant="outline" size="xs" onClick={() => load(true)}>
              {t("ui.schemaTree.tryAgain")}
            </Button>
          </div>
        ) : !tree ? (
          // Placeholder rows while loading
          <div className="flex flex-col gap-1.5 px-1.5 py-1">
            {[70, 55, 80, 45, 65].map((w, i) => (
              <div key={i} className="h-4 animate-pulse rounded bg-muted" style={{ width: `${w}%` }} />
            ))}
          </div>
        ) : total === 0 ? (
          <div className="flex flex-col items-center gap-2 px-3 py-10 text-center">
            <span className="flex size-9 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <Database size={16} />
            </span>
            <p className="text-[13px] font-medium">{t("ui.schemaTree.noTables")}</p>
            <p className="text-xs text-muted-foreground">{t("ui.schemaTree.emptyHint")}</p>
            {!readOnly && (
              <Button variant="outline" size="xs" className="mt-1" onClick={() => openDocument({ kind: "create-table", schema: defaultSchema })}>
                <Plus /> {t("ui.schemaTree.createATable")}
              </Button>
            )}
          </div>
        ) : query && shown === 0 ? (
          <div className="flex flex-col items-center gap-2 px-3 py-8 text-center">
            <p className="text-xs text-muted-foreground">{t("ui.schemaTree.nothingMatches", { query })}</p>
            <Button variant="ghost-muted" size="xs" onClick={() => setFilter("")}>
              {t("ui.schemaTree.clearTheFilter")}
            </Button>
          </div>
        ) : (
          <>
            {pinnedObjects.length > 0 && (
              <div className="mb-1">
                <GroupRow
                  depth={0}
                  open={!expanded.has("pinned-closed")}
                  onToggle={() => toggle("pinned-closed")}
                  icon={<Pin size={13} className="shrink-0 text-primary/80" />}
                  label={t("ui.schemaTree.pinned")}
                />
                {!expanded.has("pinned-closed") &&
                  pinnedObjects.map(({ schema, object }) =>
                    objectRow(schema, object, 1, CATEGORIES.find((c) => c.kinds.includes(object.kind))!, multiSchema),
                  )}
              </div>
            )}

            {tree.schemas.map((schema) => {
              const key = `s:${schema.name}`;
              const count = schema.objects.filter((o) => matches(o.name)).length;
              // While filtering, schemas without a match step aside
              if (query && count === 0) return null;
              return (
                <div key={schema.name}>
                  <GroupRow
                    depth={0}
                    open={isOpen(key)}
                    onToggle={() => toggle(key)}
                    icon={<Folder size={15} className="shrink-0 text-warning-foreground/80" />}
                    label={schema.name}
                    action={newTableButton(schema.name)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setSchemaMenu({ x: e.clientX, y: e.clientY, schema: schema.name });
                    }}
                  />
                  {isOpen(key) &&
                    (schema.objects.length === 0 ? (
                      <div className="py-1 pl-9 text-xs text-muted-foreground/70">{t("ui.schemaTree.emptySchema")}</div>
                    ) : (
                      categoryGroups(schema.name, schema.objects, 1)
                    ))}
                </div>
              );
            })}
          </>
        )}

        {/* Extensions belong to the database, not to one schema */}
        {tree && !query && capabilities.extensions && (
          <div className="mt-1">
            <GroupRow
              depth={0}
              open={expanded.has("extensions")}
              onToggle={toggleExtensions}
              icon={<Puzzle size={14} className="shrink-0 text-orange-500/80" />}
              label={t("ui.schemaTree.extensions")}
            />
            {expanded.has("extensions") &&
              (extensions === null ? (
                <div className="py-1 pl-9 text-xs text-muted-foreground/70">{t("ui.schemaTree.loading")}</div>
              ) : extensions.length === 0 ? (
                <div className="py-1 pl-9 text-xs text-muted-foreground/70">{t("ui.schemaTree.noneInstalled")}</div>
              ) : (
                extensions.map((ext) => (
                  <div key={ext.name} className="flex h-7 items-center gap-2 rounded-md pl-9 pr-2 text-sm text-muted-foreground">
                    <Puzzle size={13} className="shrink-0 text-orange-500/70" />
                    <span className="truncate">{ext.name}</span>
                    <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground/70">{ext.version}</span>
                  </div>
                ))
              ))}
          </div>
        )}
      </div>

      <ContextMenu position={menu} items={menu ? menuItems(menu.target) : []} onClose={() => setMenu(null)} />
      <ContextMenu position={plainMenu} items={plainMenu?.items ?? []} onClose={() => setPlainMenu(null)} />
      <ContextMenu position={schemaMenu} items={schemaMenu ? schemaMenuItems(schemaMenu.schema) : []} onClose={() => setSchemaMenu(null)} />
      <ContainerDialog request={container} onRun={(options) => runContainer(container!, options)} onClose={() => setContainer(null)} />
      <RelationActionDialog
        action={action?.action ?? null}
        target={action?.target ?? null}
        onRun={(options) => runAction(action!, options)}
        onClose={() => setAction(null)}
      />
    </div>
  );
}
