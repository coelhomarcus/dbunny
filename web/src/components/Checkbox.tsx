import { Check, Minus } from "lucide-react";

interface CheckboxProps {
  checked: boolean;
  indeterminate?: boolean;
  onChange: () => void;
  className?: string;
}

export default function Checkbox({ checked, indeterminate, onChange, className = "" }: CheckboxProps) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={indeterminate ? "mixed" : checked}
      onClick={(e) => { e.stopPropagation(); onChange(); }}
      className={`w-3.5 h-3.5 shrink-0 rounded-sm border transition-colors flex items-center justify-center cursor-pointer ${
        checked || indeterminate
          ? "border-green-500 text-green-400"
          : "border-zinc-600 hover:border-zinc-400 text-transparent"
      } bg-transparent ${className}`}
    >
      {indeterminate ? (
        <Minus size={9} strokeWidth={2.5} />
      ) : checked ? (
        <Check size={9} strokeWidth={2.5} />
      ) : null}
    </button>
  );
}
