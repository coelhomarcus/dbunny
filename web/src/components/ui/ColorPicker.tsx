import { useState, type KeyboardEvent, type PointerEvent } from "react";
import { clamp, hexToHsv, hsvToHex, normalizeHex, type Hsv } from "../../lib/color";
import { cn } from "../../lib/cn";
import Input from "./Input";
import { useT } from "../../i18n";

/** Fraction (0..1) of the pointer position inside an element, per axis. */
function pointerFraction(e: PointerEvent<HTMLElement>) {
  const rect = e.currentTarget.getBoundingClientRect();
  return {
    x: clamp((e.clientX - rect.left) / rect.width),
    y: clamp((e.clientY - rect.top) / rect.height),
  };
}

/** Arrow-key step: 1% (or 1°), 10x with Shift. */
function keyStep(e: KeyboardEvent, unit: number) {
  return (e.shiftKey ? 10 : 1) * unit;
}

const thumb =
  "pointer-events-none absolute size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.25),0_1px_3px_rgb(0_0_0/0.3)]";

/**
 * Hex colour picker: saturation/brightness area, hue slider and a hex field.
 * Works with pointer drag and arrow keys.
 */
export default function ColorPicker({
  value,
  onChange,
  className,
}: {
  value: string;
  onChange: (hex: string) => void;
  className?: string;
}) {
  const t = useT();
  // HSV kept locally: converting through hex would lose the hue on greys/black
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value));
  const [hexDraft, setHexDraft] = useState<string | null>(null);
  const [prevValue, setPrevValue] = useState(value);

  // The value changed from outside (e.g. a preset swatch): resync. Values we
  // emitted ourselves already match `hsv`, so the hue we picked is kept.
  if (value !== prevValue) {
    setPrevValue(value);
    if (normalizeHex(value) !== hsvToHex(hsv)) setHsv(hexToHsv(value));
  }

  function update(next: Hsv) {
    setHsv(next);
    setHexDraft(null);
    onChange(hsvToHex(next));
  }

  function onAreaPointer(e: PointerEvent<HTMLDivElement>) {
    if (e.type === "pointerdown") e.currentTarget.setPointerCapture(e.pointerId);
    else if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    const { x, y } = pointerFraction(e);
    update({ ...hsv, s: x, v: 1 - y });
  }

  function onHuePointer(e: PointerEvent<HTMLDivElement>) {
    if (e.type === "pointerdown") e.currentTarget.setPointerCapture(e.pointerId);
    else if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    update({ ...hsv, h: Math.min(359.9, pointerFraction(e).x * 360) });
  }

  function onAreaKey(e: KeyboardEvent<HTMLDivElement>) {
    const step = keyStep(e, 0.01);
    const moves: Record<string, Partial<Hsv>> = {
      ArrowLeft: { s: clamp(hsv.s - step) },
      ArrowRight: { s: clamp(hsv.s + step) },
      ArrowUp: { v: clamp(hsv.v + step) },
      ArrowDown: { v: clamp(hsv.v - step) },
    };
    if (!moves[e.key]) return;
    e.preventDefault();
    update({ ...hsv, ...moves[e.key] });
  }

  function onHueKey(e: KeyboardEvent<HTMLDivElement>) {
    const step = keyStep(e, 1);
    const delta = e.key === "ArrowLeft" || e.key === "ArrowDown" ? -step : e.key === "ArrowRight" || e.key === "ArrowUp" ? step : 0;
    if (!delta) return;
    e.preventDefault();
    update({ ...hsv, h: (hsv.h + delta + 360) % 360 });
  }

  const hex = hsvToHex(hsv);
  const hueColor = hsvToHex({ h: hsv.h, s: 1, v: 1 });
  const hexInvalid = hexDraft !== null && normalizeHex(hexDraft) === null;

  return (
    <div className={cn("flex w-56 flex-col gap-3", className)}>
      {/* Saturation (x) / brightness (y) */}
      <div
        role="slider"
        tabIndex={0}
        aria-label={t("ui.colorPicker.saturationAndBrightness")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(hsv.s * 100)}
        aria-valuetext={t("ui.colorPicker.valueText", { saturation: Math.round(hsv.s * 100), brightness: Math.round(hsv.v * 100) })}
        onPointerDown={onAreaPointer}
        onPointerMove={onAreaPointer}
        onKeyDown={onAreaKey}
        className="relative h-36 w-full cursor-crosshair touch-none rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        style={{
          backgroundColor: hueColor,
          backgroundImage: "linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent)",
        }}
      >
        <div className="pointer-events-none absolute inset-0 rounded-lg shadow-[inset_0_0_0_1px_rgb(0_0_0/0.08)]" />
        <span className={thumb} style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, backgroundColor: hex }} />
      </div>

      {/* Hue */}
      <div
        role="slider"
        tabIndex={0}
        aria-label={t("ui.colorPicker.hue")}
        aria-valuemin={0}
        aria-valuemax={360}
        aria-valuenow={Math.round(hsv.h)}
        onPointerDown={onHuePointer}
        onPointerMove={onHuePointer}
        onKeyDown={onHueKey}
        className="relative h-3 w-full cursor-pointer touch-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        style={{
          background: "linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)",
        }}
      >
        <span className={thumb} style={{ left: `${(hsv.h / 360) * 100}%`, top: "50%", backgroundColor: hueColor }} />
      </div>

      {/* Hex */}
      <div className="flex items-center gap-2">
        <span className="size-8 shrink-0 rounded-lg shadow-[inset_0_0_0_1px_rgb(0_0_0/0.1)]" style={{ backgroundColor: hex }} />
        <Input
          aria-label={t("ui.colorPicker.hexColor")}
          value={hexDraft ?? hex.toUpperCase()}
          onChange={(e) => {
            setHexDraft(e.target.value);
            const normalized = normalizeHex(e.target.value);
            if (normalized) {
              setHsv(hexToHsv(normalized));
              onChange(normalized);
            }
          }}
          onBlur={() => setHexDraft(null)}
          spellCheck={false}
          maxLength={7}
          className="font-mono text-xs uppercase"
          aria-invalid={hexInvalid}
        />
      </div>
    </div>
  );
}
