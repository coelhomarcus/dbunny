interface EditableCellProps {
  value: unknown;
  displayValue: unknown;
  isDirty: boolean;
  isEditing: boolean;
  hasNoPk: boolean;
  editValue: string;
  /** The field currently holds SQL NULL (as opposed to an empty string). */
  editIsNull: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
  onStartEdit: () => void;
  onEditValueChange: (v: string) => void;
  onSetNull: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

const MAX_PREVIEW = 300;

function renderValue(value: unknown) {
  if (value === null)
    return <span className="text-muted-foreground/70 italic">NULL</span>;
  const str = typeof value === "object" ? JSON.stringify(value) : String(value);
  if (str.length > MAX_PREVIEW) return str.slice(0, MAX_PREVIEW) + "…";
  return str;
}

export default function EditableCell({
  displayValue, isDirty, isEditing, hasNoPk,
  editValue, editIsNull, inputRef,
  onStartEdit, onEditValueChange, onSetNull, onConfirm, onCancel,
}: EditableCellProps) {
  return (
    <td
      className={`relative p-0 overflow-hidden border-b border-r border-border ${isDirty ? "bg-warning/8" : ""}`}
      onClick={() => !isEditing && onStartEdit()}
    >
      <div
        className={`px-3 py-1.5 truncate overflow-hidden ${isEditing ? "invisible" : ""} ${
          isDirty ? "text-warning-foreground" : "text-foreground"
        } ${!hasNoPk && !isEditing ? "cursor-text" : ""}`}
      >
        {renderValue(displayValue)}
      </div>
      {isEditing && (
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
            className="absolute inset-0 px-3 pr-14 bg-card text-foreground text-sm outline-none ring-1 ring-inset ring-primary/60 placeholder:italic placeholder:text-muted-foreground/70"
          />
          {/* mousedown is prevented so the input keeps focus (a blur would confirm the edit) */}
          <button
            type="button"
            tabIndex={-1}
            onMouseDown={(e) => {
              e.preventDefault();
              onSetNull();
            }}
            className={`absolute right-1.5 top-1/2 z-10 -translate-y-1/2 rounded px-1.5 py-0.5 text-[11px] font-medium cursor-pointer transition-colors ${
              editIsNull ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground hover:bg-accent hover:text-foreground"
            }`}
          >
            NULL
          </button>
        </>
      )}
    </td>
  );
}
