import { createElement } from "react";
import type { LucideProps } from "lucide-react";
import { connectionIcon } from "../lib/connectionIcons";

/** Draws a saved connection's icon (a fixed component, so it isn't recreated on render). */
export default function ConnectionGlyph({ icon, ...props }: { icon: string | null | undefined } & LucideProps) {
  return createElement(connectionIcon(icon), props);
}
