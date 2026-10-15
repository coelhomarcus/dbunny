import { Monitor, Moon, Sun } from "lucide-react";
import { useTheme, type ThemePreference } from "../../lib/theme";
import Button from "./Button";
import Tooltip from "../Tooltip";

const NEXT: Record<ThemePreference, ThemePreference> = { system: "light", light: "dark", dark: "system" };
const LABEL: Record<ThemePreference, string> = { system: "Theme: System", light: "Theme: Light", dark: "Theme: Dark" };

/** Cycles System → Light → Dark. */
export default function ThemeToggle({ side = "top" }: { side?: "top" | "bottom" | "left" | "right" }) {
  const { preference, setPreference } = useTheme();
  const Icon = preference === "system" ? Monitor : preference === "light" ? Sun : Moon;

  return (
    <Tooltip label={LABEL[preference]} side={side}>
      <Button variant="ghost-muted" size="icon-sm" aria-label={LABEL[preference]} onClick={() => setPreference(NEXT[preference])}>
        <Icon />
      </Button>
    </Tooltip>
  );
}
