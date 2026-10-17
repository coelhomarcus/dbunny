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
  Plus,
  Server,
  Shapes,
  Split,
  Table2,
  Terminal,
  Users,
  X,
  Zap,
  type LucideProps,
} from "lucide-react";
import Popover from "../ui/Popover";
import { useDocuments } from "../../contexts/DocumentsContext";
import { useCapabilities } from "../../hooks/useCapabilities";
import { cn } from "../../lib/cn";
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
  }
}

/** The connection's document tabs: icon, name, and close. */
export default function DocumentTabBar() {
  const { documents, openDocument, focusDocument, closeDocument, renameQuery, dirtyIds } = useDocuments();
  const capabilities = useCapabilities();
  const [renamingId, setRenamingId] = useState<string | null>(null);
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
                className="mb-1 h-7 w-36 shrink-0 rounded-md border border-ring bg-card px-2 text-[13px] outline-none ring-3 ring-ring/20"
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
                  closeDocument(doc.id);
                }
              }}
              className={cn(
                "joined-tab group flex shrink-0 cursor-pointer items-center gap-1.5 pl-2.5 pr-1 pt-1.5 text-[13px] whitespace-nowrap outline-none",
                active ? "pb-[7px] text-foreground" : "mb-1 rounded-lg pb-0.5 text-muted-foreground hover:bg-accent hover:text-foreground",
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
                  closeDocument(doc.id);
                }}
                className={cn(
                  "mr-0.5 flex size-5 shrink-0 items-center justify-center rounded hover:bg-foreground/10 hover:text-foreground",
                  active ? "text-muted-foreground" : "text-transparent group-hover:text-muted-foreground",
                )}
              >
                <X size={12} />
              </span>
            </button>
          );
        })}
      </div>

      <div className="mb-1 flex shrink-0 items-center pr-1">
      <button
        type="button"
        aria-label="New query"
        onClick={() => openDocument({ kind: "query" })}
        className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <Plus size={15} />
      </button>

      {capabilities.admin && (
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
            <MenuItem
              icon={Activity}
              iconClass="text-emerald-500"
              label="Activity"
              onSelect={() => {
                openDocument({ kind: "activity" });
                setServerOpen(false);
              }}
            />
            <MenuItem
              icon={Users}
              iconClass="text-indigo-500"
              label="Roles"
              onSelect={() => {
                openDocument({ kind: "roles" });
                setServerOpen(false);
              }}
            />
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
