interface EditableCellProps {
  value: unknown;
  displayValue: unknown;
  isDirty: boolean;
  isEditing: boolean;
  hasNoPk: boolean;
  editValue: string;
  inputRef: React.RefObject<HTMLInputElement | null>;
  onStartEdit: () => void;
  onEditValueChange: (v: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
}

const MAX_PREVIEW = 300;

function renderValue(value: unknown) {
  if (value === null)
    return <span className="text-zinc-600 italic">NULL</span>;
  const str = typeof value === "object" ? JSON.stringify(value) : String(value);
  if (str.length > MAX_PREVIEW) return str.slice(0, MAX_PREVIEW) + "…";
  return str;
}

export default function EditableCell({
  displayValue, isDirty, isEditing, hasNoPk,
  editValue, inputRef,
  onStartEdit, onEditValueChange, onConfirm, onCancel,
}: EditableCellProps) {
  return (
    <td
      className={`relative p-0 overflow-hidden border-b border-r border-zinc-800/40 ${isDirty ? "border-l-2 border-l-amber-400/60" : ""}`}
      onClick={() => !isEditing && onStartEdit()}
    >
      <div
        className={`px-3 py-1.5 truncate overflow-hidden ${isEditing ? "invisible" : ""} ${
          isDirty ? "text-amber-300" : "text-zinc-300"
        } ${!hasNoPk && !isEditing ? "cursor-text" : ""}`}
      >
        {renderValue(displayValue)}
      </div>
      {isEditing && (
        <input
          ref={inputRef}
          value={editValue}
          onChange={(e) => onEditValueChange(e.target.value)}
          onBlur={onConfirm}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              onConfirm();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              onCancel();
            }
          }}
          className="absolute inset-0 px-3 bg-zinc-800 text-white text-sm outline-none ring-1 ring-inset ring-green-500/60"
        />
      )}
    </td>
  );
}
