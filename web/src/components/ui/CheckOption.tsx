import Checkbox from "../Checkbox";

/** A checkbox with a label and an optional hint under it, for dialog options. */
export default function CheckOption({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string }) {
  return (
    // A div, not a label: a label would also click the checkbox button and toggle twice
    <div className="flex cursor-pointer items-start gap-2.5 select-none" onClick={() => onChange(!checked)}>
      <Checkbox checked={checked} onChange={() => onChange(!checked)} className="mt-0.5" />
      <span>
        <span className="block text-[13px] font-medium">{label}</span>
        {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      </span>
    </div>
  );
}
