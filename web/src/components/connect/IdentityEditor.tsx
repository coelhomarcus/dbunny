import { useRef, useState, type Ref } from "react";
import { ChevronDown } from "lucide-react";
import { COLOR_PRESETS, type Identity } from "../../lib/connectionDraft";
import { CONNECTION_ICONS } from "../../lib/connectionIcons";
import { cn } from "../../lib/cn";
import ConnectionGlyph from "../ConnectionGlyph";
import Field from "../ui/Field";
import Input from "../ui/Input";
import Popover from "../ui/Popover";
import ColorPicker from "../ui/ColorPicker";
import Tooltip from "../Tooltip";

/** A swatch per preset, then the custom colour: in a popover (`custom="popover"`) or opened below (`"inline"`). */
function ColorChoices({ color, onChange, custom }: { color: string; onChange: (color: string) => void; custom: "popover" | "inline" }) {
  const [pickerOpen, setPickerOpen] = useState(false);
  const customRef = useRef<HTMLButtonElement>(null);
  const isCustom = !COLOR_PRESETS.includes(color);

  return (
    <>
      <div className="flex items-center gap-2">
        {COLOR_PRESETS.map((preset) => (
          <button
            key={preset}
            type="button"
            aria-label={`Color ${preset}`}
            aria-pressed={color === preset}
            onClick={() => onChange(preset)}
            className={cn(
              "size-5 rounded-full cursor-pointer outline-none transition-shadow focus-visible:ring-2 focus-visible:ring-ring/50",
              color === preset && "ring-2 ring-offset-2 ring-offset-card",
            )}
            style={{ backgroundColor: preset, ["--tw-ring-color" as string]: preset }}
          />
        ))}
        <span className="mx-0.5 h-4 w-px bg-border" />
        <button
          ref={customRef}
          type="button"
          aria-label="Custom color"
          aria-expanded={pickerOpen}
          onClick={() => setPickerOpen((v) => !v)}
          className={cn(
            "size-5 rounded-full cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
            (isCustom || pickerOpen) && "ring-2 ring-offset-2 ring-offset-card",
          )}
          style={{
            background: isCustom ? color : "conic-gradient(#ef4444, #eab308, #22c55e, #3b82f6, #8b5cf6, #ef4444)",
            ["--tw-ring-color" as string]: isCustom ? color : "var(--ring)",
          }}
        />
        {custom === "popover" && (
          <Popover open={pickerOpen} onClose={() => setPickerOpen(false)} anchorRef={customRef} className="p-3">
            <ColorPicker value={color} onChange={onChange} />
          </Popover>
        )}
      </div>
      {custom === "inline" && pickerOpen && (
        <div className="mt-3">
          <ColorPicker value={color} onChange={onChange} />
        </div>
      )}
    </>
  );
}

function IconChoices({ icon, color, onChange }: { icon: string; color: string; onChange: (icon: string) => void }) {
  return (
    <div role="radiogroup" aria-label="Icon" className="flex flex-wrap gap-1">
      {CONNECTION_ICONS.map(({ id, label, Icon }, i) => {
        // The first one is the default, stored as ""
        const value = i === 0 ? "" : id;
        const selected = icon === value;
        return (
          <Tooltip key={id} label={label} side="top">
            <button
              type="button"
              role="radio"
              aria-label={label}
              aria-checked={selected}
              onClick={() => onChange(value)}
              className={cn(
                "flex size-8 items-center justify-center rounded-lg text-muted-foreground cursor-pointer outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50",
                selected && "hover:bg-transparent",
              )}
              style={
                selected
                  ? {
                      color,
                      backgroundColor: `color-mix(in srgb, ${color} 16%, transparent)`,
                      boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 40%, transparent)`,
                    }
                  : undefined
              }
            >
              <Icon size={15} />
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}

/**
 * A connection's name, colour and icon. "stacked" lays them out as form fields; "header" is
 * one line for the top of the connection modal: the icon tile (opens colour and icon) and the name.
 */
export default function IdentityEditor({
  identity,
  onChange,
  nameRef,
  nameInvalid,
  layout,
}: {
  identity: Identity;
  onChange: (next: Identity) => void;
  nameRef?: Ref<HTMLInputElement>;
  /** Marks the name as needed (saving without one). */
  nameInvalid?: boolean;
  layout: "stacked" | "header";
}) {
  const [lookOpen, setLookOpen] = useState(false);
  const tileRef = useRef<HTMLButtonElement>(null);
  const set = (changes: Partial<Identity>) => onChange({ ...identity, ...changes });

  if (layout === "stacked") {
    return (
      <>
        <Field label="Name" htmlFor="conn-name">
          <Input
            ref={nameRef}
            id="conn-name"
            value={identity.name}
            onChange={(e) => set({ name: e.target.value })}
            placeholder="e.g. Production"
            spellCheck={false}
            aria-invalid={nameInvalid || undefined}
          />
        </Field>
        <Field label="Color" hint="Tag environments, e.g. red for production.">
          <ColorChoices color={identity.color} onChange={(color) => set({ color })} custom="popover" />
        </Field>
        <Field label="Icon" hint="Shown in the sidebar and on the tab, in the colour above.">
          <IconChoices icon={identity.icon} color={identity.color} onChange={(icon) => set({ icon })} />
        </Field>
      </>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 items-center gap-2.5">
      <Tooltip label="Color and icon" side="bottom">
        <button
          ref={tileRef}
          type="button"
          aria-label="Color and icon"
          aria-expanded={lookOpen}
          onClick={() => setLookOpen((v) => !v)}
          className="group flex h-9 shrink-0 items-center gap-0.5 rounded-lg pl-2 pr-1 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring/50 [&_svg]:shrink-0"
          style={{
            color: identity.color,
            backgroundColor: `color-mix(in srgb, ${identity.color} 16%, transparent)`,
            boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${identity.color} 22%, transparent)`,
          }}
        >
          <ConnectionGlyph icon={identity.icon} size={17} />
          <ChevronDown size={12} className="opacity-60 transition-opacity group-hover:opacity-100" />
        </button>
      </Tooltip>
      <Popover open={lookOpen} onClose={() => setLookOpen(false)} anchorRef={tileRef} className="w-[19rem] p-3">
        <div className="mb-2 text-xs font-medium text-muted-foreground">Color</div>
        <ColorChoices color={identity.color} onChange={(color) => set({ color })} custom="inline" />
        <div className="mb-2 mt-4 text-xs font-medium text-muted-foreground">Icon</div>
        <IconChoices icon={identity.icon} color={identity.color} onChange={(icon) => set({ icon })} />
      </Popover>
      <input
        ref={nameRef}
        id="conn-name"
        aria-label="Connection name"
        value={identity.name}
        onChange={(e) => set({ name: e.target.value })}
        placeholder="Untitled connection"
        spellCheck={false}
        autoComplete="off"
        aria-invalid={nameInvalid || undefined}
        className="h-9 min-w-0 flex-1 rounded-lg bg-transparent px-2 text-[15px] font-semibold text-foreground outline-none placeholder:text-muted-foreground/70 hover:bg-muted focus-visible:bg-muted aria-[invalid=true]:ring-2 aria-[invalid=true]:ring-destructive/40"
      />
    </div>
  );
}
