/** HSV with h in [0, 360), s and v in [0, 1]. */
export interface Hsv {
  h: number;
  s: number;
  v: number;
}

const clamp = (n: number, min = 0, max = 1) => Math.min(max, Math.max(min, n));

/** Accepts #rgb or #rrggbb (with or without #); returns null if invalid. */
export function normalizeHex(input: string): string | null {
  const hex = input.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(hex)) return `#${hex.split("").map((c) => c + c).join("")}`;
  if (/^[0-9a-f]{6}$/.test(hex)) return `#${hex}`;
  return null;
}

export function hexToHsv(input: string): Hsv {
  const hex = normalizeHex(input) ?? "#000000";
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const max = Math.max(r, g, b);
  const delta = max - Math.min(r, g, b);

  let h = 0;
  if (delta !== 0) {
    if (max === r) h = ((g - b) / delta) % 6;
    else if (max === g) h = (b - r) / delta + 2;
    else h = (r - g) / delta + 4;
    h = (h * 60 + 360) % 360;
  }
  return { h, s: max === 0 ? 0 : delta / max, v: max };
}

export function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  const toHex = (x: number) => Math.round(clamp(x) * 255).toString(16).padStart(2, "0");
  return `#${toHex(f(5))}${toHex(f(3))}${toHex(f(1))}`;
}

export { clamp };
