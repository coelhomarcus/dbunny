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
      className={`w-4 h-4 shrink-0 rounded-sm border transition-colors flex items-center justify-center cursor-pointer ${
        checked || indeterminate
          ? "border-primary bg-primary text-primary-foreground"
          : "border-input hover:border-ring text-transparent bg-transparent"
      } ${className}`}
    >
      {indeterminate ? (
        <Minus size={10} strokeWidth={2.5} />
      ) : checked ? (
        <Check size={10} strokeWidth={2.5} />
      ) : null}
    </button>
  );
}
