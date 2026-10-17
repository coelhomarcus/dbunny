import { useRef, useState, type ComponentType } from "react";
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

/** The connection's document tabs: icon, name, schema, and close. */
export default function DocumentTabBar() {
  const { documents, openDocument, focusDocument, closeDocument, renameQuery, dirtyIds } = useDocuments();
  const capabilities = useCapabilities();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [serverOpen, setServerOpen] = useState(false);
  const [listOpen, setListOpen] = useState(false);
  const serverRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-2 select-none">
      <div role="tablist" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
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
                className="h-7 w-36 shrink-0 rounded-md border border-ring bg-card px-2 text-[13px] outline-none ring-3 ring-ring/20"
              />
            );
          }
          return (
            <button
              key={doc.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => focusDocument(doc.id)}
              onDoubleClick={() => {
                if (doc.kind === "query") setRenamingId(doc.id);
              }}
              className={cn(
                "group flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md pl-2 text-[13px] whitespace-nowrap",
                active ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <Icon size={14} className="shrink-0" />
              <span className="max-w-40 truncate">{doc.title}</span>
              {doc.schema && <span className="text-[11px] text-muted-foreground">{doc.schema}</span>}
              {dirtyIds.has(doc.id) && <span className="size-1.5 shrink-0 rounded-full bg-primary" aria-label="Unsaved changes" />}
              <span
                role="button"
                aria-label={`Close ${doc.title}`}
                onClick={(e) => {
                  e.stopPropagation();
                  closeDocument(doc.id);
                }}
                className="mr-0.5 flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
              >
                <X size={12} />
              </span>
            </button>
          );
        })}
      </div>

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
              label="Activity"
              onSelect={() => {
                openDocument({ kind: "activity" });
                setServerOpen(false);
              }}
            />
            <MenuItem
              icon={Users}
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
                <Icon size={14} className="shrink-0 text-muted-foreground" />
                <span className="truncate">{doc.title}</span>
                {doc.schema && <span className="text-[11px] text-muted-foreground">{doc.schema}</span>}
              </button>
            );
          })
        )}
      </Popover>
    </div>
  );
}

function MenuItem({ icon: Icon, label, onSelect }: { icon: ComponentType<LucideProps>; label: string; onSelect: () => void }) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-accent"
    >
      <Icon size={14} className="shrink-0 text-muted-foreground" />
      {label}
    </button>
  );
}
