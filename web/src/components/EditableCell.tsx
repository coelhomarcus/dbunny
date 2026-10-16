import { useRef } from "react";
import { ArrowUpRight, Maximize2 } from "lucide-react";
import type { CellEditor } from "../lib/cellEdit";
import { toEditText } from "../lib/cellEdit";
import ChoiceList from "./table/ChoiceList";
import DateTimeEditor from "./table/DateTimeEditor";

interface EditableCellProps {
  value: unknown;
  displayValue: unknown;
  isDirty: boolean;
  isEditing: boolean;
  hasNoPk: boolean;
  /** How this cell is edited (inline text, a picker, a dialog, or not at all). */
  editor: CellEditor;
  nullable: boolean;
  editValue: string;
  /** The field currently holds SQL NULL (as opposed to an empty string). */
  editIsNull: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
  onStartEdit: () => void;
  onEditValueChange: (v: string) => void;
  onSetNull: () => void;
  onConfirm: () => void;
  onCommit: (isNull: boolean, text: string) => void;
  onCancel: () => void;
  /** Moves the inline edit to the larger dialog editor. */
  onExpand: () => void;
  /** Foreign key cells: opens the row this value references. */
  onFollowLink?: () => void;
  linkLabel?: string;
  onContextMenu: (e: React.MouseEvent) => void;
  /** The context menu is open on this cell. */
  menuOpen?: boolean;
}

const MAX_PREVIEW = 300;

function renderValue(value: unknown) {
  if (value === null)
    return <span className="text-muted-foreground/70 italic">NULL</span>;
  const str = typeof value === "object" ? JSON.stringify(value) : String(value);
  if (str === "") return <span className="text-muted-foreground/70 italic">empty</span>;
  // Rows stay one line high: line breaks show as a return symbol
  const oneLine = str.length > MAX_PREVIEW ? str.slice(0, MAX_PREVIEW) + "…" : str;
  return oneLine.replace(/\r?\n/g, " ↵ ");
}

// Shared look of the small buttons inside an edited cell
const inCellButton =
  "absolute top-1/2 z-10 -translate-y-1/2 rounded px-1.5 py-0.5 text-[11px] font-medium cursor-pointer transition-colors";

export default function EditableCell({
  displayValue, isDirty, isEditing, hasNoPk, editor, nullable,
  editValue, editIsNull, inputRef,
  onStartEdit, onEditValueChange, onSetNull, onConfirm, onCommit, onCancel, onExpand,
  onFollowLink, linkLabel, onContextMenu, menuOpen,
}: EditableCellProps) {
  const cellRef = useRef<HTMLTableCellElement>(null);
  const editable = !hasNoPk && editor.kind !== "readonly";
  const inlineEditing = isEditing && editor.kind === "text";

  return (
    <td
      ref={cellRef}
      className={`relative p-0 overflow-hidden border-b border-r border-border ${isDirty ? "bg-warning/8" : ""} ${
        (isEditing && !inlineEditing) || menuOpen ? "ring-1 ring-inset ring-primary/60" : ""
      }`}
      onClick={() => editable && !isEditing && onStartEdit()}
      onContextMenu={(e) => {
        // An inline edit keeps its own native menu (copy/paste in the input)
        if (inlineEditing) return;
        e.preventDefault();
        onContextMenu(e);
      }}
    >
      <div
        className={`px-3 py-1.5 truncate overflow-hidden ${inlineEditing ? "invisible" : ""} ${
          isDirty ? "text-warning-foreground" : editor.kind === "readonly" ? "text-muted-foreground" : "text-foreground"
        } ${editable && !isEditing ? (editor.kind === "text" ? "cursor-text" : "cursor-pointer") : ""}`}
      >
        {renderValue(displayValue)}
      </div>
      {onFollowLink && !isEditing && (
        <button
          type="button"
          aria-label={linkLabel}
          onClick={(e) => {
            e.stopPropagation();
            onFollowLink();
          }}
          className="absolute right-1 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded-md bg-card text-info-foreground opacity-0 shadow-xs transition-opacity cursor-pointer group-hover/row:opacity-100 hover:bg-accent focus-visible:opacity-100"
        >
          <ArrowUpRight size={13} />
        </button>
      )}
      {isEditing && editor.kind === "choice" && (
        <ChoiceList
          anchorRef={cellRef}
          options={editor.options}
          current={displayValue === null || displayValue === undefined ? null : toEditText(displayValue)}
          nullable={nullable}
          onPick={(value) => onCommit(value === null, value ?? "")}
          onCancel={onCancel}
        />
      )}
      {isEditing && editor.kind === "datetime" && (
        <DateTimeEditor
          anchorRef={cellRef}
          mode={editor.mode}
          value={displayValue}
          nullable={nullable}
          onCommit={onCommit}
          onCancel={onCancel}
        />
      )}
      {inlineEditing && (
        <>
          <input
            ref={inputRef}
            value={editValue}
            onChange={(e) => onEditValueChange(e.target.value)}
            onBlur={onConfirm}
            placeholder={editIsNull ? "NULL" : "empty"}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onConfirm();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                onCancel();
              }
              // Ctrl/Cmd+Shift+N: set the value to NULL
              if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "n") {
                e.preventDefault();
                onSetNull();
              }
            }}
            className={`absolute inset-0 px-3 bg-card text-foreground text-sm outline-none ring-1 ring-inset ring-primary/60 placeholder:italic placeholder:text-muted-foreground/70 ${
              nullable ? "pr-20" : "pr-9"
            }`}
          />
          {/* mousedown is prevented so the input keeps focus (a blur would confirm the edit) */}
          {nullable && (
            <button
              type="button"
              tabIndex={-1}
              onMouseDown={(e) => {
                e.preventDefault();
                onSetNull();
              }}
              className={`${inCellButton} right-8 ${
                editIsNull ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground hover:bg-accent hover:text-foreground"
              }`}
            >
              NULL
            </button>
          )}
          <button
            type="button"
            tabIndex={-1}
            aria-label="Open in a larger editor"
            onMouseDown={(e) => {
              e.preventDefault();
              onExpand();
            }}
            className={`${inCellButton} right-1.5 flex items-center bg-muted py-1 text-muted-foreground hover:bg-accent hover:text-foreground`}
          >
            <Maximize2 size={11} />
          </button>
        </>
      )}
    </td>
  );
}
