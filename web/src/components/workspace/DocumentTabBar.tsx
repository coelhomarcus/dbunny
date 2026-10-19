import { useEffect, useRef, useState, type ComponentType, type WheelEvent } from "react";
import {
  Activity,
  Braces,
  ChevronDown,
  Columns3,
  Eye,
  Layers,
  ListOrdered,
  ListTree,
  ArrowRightToLine,
  PenLine,
  Plus,
  Server,
  Sparkles,
  Shapes,
  Split,
  Table2,
  Terminal,
  TriangleAlert,
  Users,
  Waypoints,
  X,
  Zap,
  type LucideProps,
} from "lucide-react";
import Popover from "../ui/Popover";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu";
import ConfirmDialog from "../ui/ConfirmDialog";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useAssistant } from "../../contexts/assistant";
import { useCapabilities } from "../../hooks/useCapabilities";
import { cn } from "../../lib/cn";
import { isMac } from "../../lib/platform";
import Tooltip from "../Tooltip";
import type { DataIcon, Document } from "../../lib/documents";

const DATA_ICON: Record<DataIcon, ComponentType<LucideProps>> = {
  table: Table2,
  view: Eye,
  matview: Layers,
  partition: Split,
};

function iconFor(doc: Document): ComponentType<LucideProps> {
  switch (doc.kind) {
    case "query":
      return Terminal;
    case "data":
      return DATA_ICON[doc.icon ?? "table"];
    case "structure":
      return Columns3;
    case "activity":
      return Activity;
    case "roles":
      return Users;
    case "function":
    case "procedure":
      return Braces;
    case "sequence":
      return ListOrdered;
    case "type":
      return Shapes;
    case "index":
      return ListTree;
    case "trigger":
      return Zap;
    case "create-table":
      return Table2;
    case "diagram":
      return Waypoints;
    case "assistant":
      return Sparkles;
  }
}

/** Same colours as the explorer, so a tab matches the object it came from. */
function iconClass(doc: Document): string {
  switch (doc.kind) {
    case "query":
      return "text-amber-500";
    case "data":
      switch (doc.icon ?? "table") {
        case "view":
          return "text-info-foreground";
        case "matview":
          return "text-cyan-500";
        case "partition":
          return "text-success-foreground/70";
        default:
          return "text-success-foreground";
      }
    case "structure":
      return "text-sky-500";
    case "activity":
      return "text-emerald-500";
    case "roles":
      return "text-indigo-500";
    case "function":
    case "procedure":
      return "text-purple-500";
    case "sequence":
      return "text-teal-500";
    case "type":
      return "text-pink-500";
    case "index":
      return "text-orange-500";
    case "trigger":
      return "text-yellow-500";
    case "create-table":
      return "text-success-foreground";
    case "diagram":
      return "text-info-foreground";
    case "assistant":
      return "text-primary";
  }
}

/** The connection's document tabs: icon, name, and close. */
export default function DocumentTabBar() {
  const { documents, openDocument, focusDocument, closeDocuments, renameQuery, dirtyIds } = useDocuments();
  const capabilities = useCapabilities();
  const assistant = useAssistant();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [pendingClose, setPendingClose] = useState<string[] | null>(null);
  const [serverOpen, setServerOpen] = useState(false);
  const [listOpen, setListOpen] = useState(false);
  const serverRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLButtonElement>(null);
  const tabsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const bar = tabsRef.current;
    const tab = bar?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
    if (!bar || !tab) return;
    const barRect = bar.getBoundingClientRect();
    const tabRect = tab.getBoundingClientRect();
    if (tabRect.left < barRect.left) bar.scrollLeft -= barRect.left - tabRect.left;
    else if (tabRect.right > barRect.right) bar.scrollLeft += tabRect.right - barRect.right;
  }, [documents.activeId]);

  function askClose(ids: string[]) {
    const unique = [...new Set(ids)].filter((id) => documents.items.some((doc) => doc.id === id));
    if (unique.length === 0) return;
    if (unique.some((id) => dirtyIds.has(id))) setPendingClose(unique);
    else closeDocuments(unique);
  }

  const menuDoc = menu ? documents.items.find((doc) => doc.id === menu.id) : undefined;
  const menuIndex = menuDoc ? documents.items.findIndex((doc) => doc.id === menuDoc.id) : -1;
  const menuItems: ContextMenuItem[] = menuDoc
    ? [
        { label: "Close", icon: <X />, onSelect: () => askClose([menuDoc.id]) },
        {
          label: "Close others",
          icon: <X />,
          disabled: documents.items.length < 2,
          onSelect: () => askClose(documents.items.filter((doc) => doc.id !== menuDoc.id).map((doc) => doc.id)),
        },
        {
          label: "Close tabs to the right",
          icon: <ArrowRightToLine />,
          disabled: menuIndex < 0 || menuIndex === documents.items.length - 1,
          onSelect: () => askClose(documents.items.slice(menuIndex + 1).map((doc) => doc.id)),
        },
        { label: "Close all", icon: <X />, onSelect: () => askClose(documents.items.map((doc) => doc.id)) },
        ...(menuDoc.kind === "query"
          ? (["separator", { label: "Rename", icon: <PenLine />, onSelect: () => setRenamingId(menuDoc.id) }] as ContextMenuItem[])
          : []),
      ]
    : [];
  const pendingTitles = (pendingClose ?? []).filter((id) => dirtyIds.has(id)).map((id) => documents.items.find((doc) => doc.id === id)?.title).filter(Boolean);

  function onTabsWheel(e: WheelEvent<HTMLDivElement>) {
    const bar = e.currentTarget;
    if (bar.scrollWidth <= bar.clientWidth || e.deltaX !== 0 || e.deltaY === 0) return;
    bar.scrollLeft += e.deltaY;
  }

  return (
    <div className="tab-strip document-tabs flex h-10 shrink-0 items-end select-none">
      <div
        ref={tabsRef}
        role="tablist"
        onWheel={onTabsWheel}
        className="scrollbar-none flex min-w-0 flex-1 items-end gap-1 overflow-x-auto px-2"
      >
        {documents.items.map((doc) => {
          const active = doc.id === documents.activeId;
          const Icon = iconFor(doc);
          if (renamingId === doc.id) {
            return (
              <input
                key={doc.id}
                autoFocus
                defaultValue={doc.title}
                aria-label="Tab name"
                onFocus={(e) => e.currentTarget.select()}
                onBlur={(e) => {
                  renameQuery(doc.id, e.currentTarget.value);
                  setRenamingId(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    renameQuery(doc.id, e.currentTarget.value);
                    setRenamingId(null);
                  }
                  if (e.key === "Escape") setRenamingId(null);
                }}
                className="mb-1.5 h-7 w-36 shrink-0 rounded-md border border-ring bg-card px-2 text-[13px] outline-none ring-3 ring-ring/20"
              />
            );
          }
          return (
            <button
              key={doc.id}
              type="button"
              role="tab"
              aria-selected={active}
              data-active={active}
              onClick={() => focusDocument(doc.id)}
              onDoubleClick={() => {
                if (doc.kind === "query") setRenamingId(doc.id);
              }}
              onAuxClick={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  askClose([doc.id]);
                }
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, id: doc.id });
              }}
              className={cn(
                "joined-tab group flex h-[34px] shrink-0 cursor-pointer items-center pb-1.5 text-[13px] whitespace-nowrap outline-none",
                active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
              )}
            >
              <span
                className={cn(
                  "flex h-full min-w-0 items-center gap-1.5 pl-2.5 pr-1",
                  !active && "rounded-lg group-hover:bg-accent",
                )}
              >
              <Icon size={14} className={cn("shrink-0", iconClass(doc))} />
              <span className="max-w-40 truncate">{doc.title}</span>
              {dirtyIds.has(doc.id) && <span className="size-1.5 shrink-0 rounded-full bg-primary" aria-label="Unsaved changes" />}
              <span
                role="button"
                aria-label={`Close ${doc.title}`}
                onClick={(e) => {
                  e.stopPropagation();
                  askClose([doc.id]);
                }}
                className={cn(
                  "mr-0.5 flex size-5 shrink-0 items-center justify-center rounded hover:bg-foreground/10 hover:text-foreground",
                  active ? "text-muted-foreground" : "text-transparent group-hover:text-muted-foreground",
                )}
              >
                <X size={12} />
              </span>
              </span>
            </button>
          );
        })}
      </div>

      <div className="mb-1.5 flex shrink-0 items-center pr-1">
      <button
        type="button"
        aria-label="New query"
        onClick={() => openDocument({ kind: "query" })}
        className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <Plus size={15} />
      </button>

      {assistant.available && (
        <Tooltip label={`Assistant (${isMac ? "⌘I" : "Ctrl+I"})`} side="bottom">
          <button
            type="button"
            aria-label="Assistant"
            aria-pressed={assistant.panelOpen}
            onClick={assistant.toggle}
            className={cn(
              "flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground",
              assistant.panelOpen && "bg-muted text-foreground",
            )}
          >
            <Sparkles size={15} />
          </button>
        </Tooltip>
      )}

      {(capabilities.activity || capabilities.roles) && (
        <>
          <button
            ref={serverRef}
            type="button"
            aria-label="Open a server screen"
            aria-expanded={serverOpen}
            onClick={() => {
              setListOpen(false);
              setServerOpen((open) => !open);
            }}
            className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <Server size={15} />
          </button>
          <Popover open={serverOpen} onClose={() => setServerOpen(false)} anchorRef={serverRef}>
            {capabilities.activity && (
              <MenuItem
                icon={Activity}
                iconClass="text-emerald-500"
                label="Activity"
                onSelect={() => {
                  openDocument({ kind: "activity" });
                  setServerOpen(false);
                }}
              />
            )}
            {capabilities.roles && (
              <MenuItem
                icon={Users}
                iconClass="text-indigo-500"
                label="Roles"
                onSelect={() => {
                  openDocument({ kind: "roles" });
                  setServerOpen(false);
                }}
              />
            )}
          </Popover>
        </>
      )}

      <button
        ref={listRef}
        type="button"
        aria-label="Show open tabs"
        aria-expanded={listOpen}
        onClick={() => {
          setServerOpen(false);
          setListOpen((open) => !open);
        }}
        className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <ChevronDown size={15} />
      </button>
      </div>
      <Popover open={listOpen} onClose={() => setListOpen(false)} anchorRef={listRef}>
        {documents.items.length === 0 ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">Nothing is open.</div>
        ) : (
          documents.items.map((doc) => {
            const Icon = iconFor(doc);
            return (
              <button
                key={doc.id}
                type="button"
                onClick={() => {
                  focusDocument(doc.id);
                  setListOpen(false);
                }}
                className="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-accent"
              >
                <Icon size={14} className={cn("shrink-0", iconClass(doc))} />
                <span className="truncate">{doc.title}</span>
              </button>
            );
          })
        )}
      </Popover>
      <ContextMenu position={menu} items={menuItems} onClose={() => setMenu(null)} />
      <ConfirmDialog
        open={pendingClose !== null}
        onClose={() => setPendingClose(null)}
        onConfirm={() => {
          if (pendingClose) closeDocuments(pendingClose);
          setPendingClose(null);
        }}
        icon={<TriangleAlert />}
        tone="warning"
        title={pendingTitles.length > 1 ? `Close ${pendingTitles.length} tabs without saving?` : "Close without saving?"}
        description={
          pendingTitles.length > 1
            ? `${pendingTitles.join(", ")} have unsaved changes.`
            : `${pendingTitles[0] ?? "This tab"} has unsaved changes.`
        }
        confirmLabel="Discard and close"
      />
    </div>
  );
}

function MenuItem({
  icon: Icon,
  iconClass,
  label,
  onSelect,
}: {
  icon: ComponentType<LucideProps>;
  iconClass: string;
  label: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-accent"
    >
      <Icon size={14} className={cn("shrink-0", iconClass)} />
      {label}
    </button>
  );
}
