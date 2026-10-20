import { useState, useRef, useEffect } from "react";
import { Download } from "lucide-react";
import Spinner from "./ui/Spinner";
import { exportData, type ExportFormat } from "../lib/exportData";
import type { QueryColumn } from "@/types";
import Button from "./ui/Button";
import Tooltip from "./Tooltip";
import { useT } from "../i18n";

interface ExportMenuProps {
  columns: QueryColumn[];
  rows: unknown[][];
  defaultName: string;
  side?: "up" | "down";
  totalRows?: number;
  /** The total is an estimate: shown with a ~ and counted exactly before exporting. */
  totalRowsEstimated?: boolean;
  onFetchAll?: () => Promise<{ columns: QueryColumn[]; rows: unknown[][] }>;
  /** Smaller trigger, for toolbars at the bottom of a view. */
  compact?: boolean;
}

const formats: { value: ExportFormat; label: string }[] = [
  { value: "csv", label: "CSV" },
  { value: "json", label: "JSON" },
];

export default function ExportMenu({
  columns, rows, defaultName,
  side = "down", totalRows, totalRowsEstimated, onFetchAll, compact,
}: ExportMenuProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  async function handleExport(format: ExportFormat) {
    setOpen(false);
    await exportData(columns, rows, format, defaultName);
  }

  async function handleExportAll(format: ExportFormat) {
    if (!onFetchAll) return;
    setLoading(true);
    try {
      const all = await onFetchAll();
      setOpen(false);
      await exportData(all.columns, all.rows, format, defaultName);
    } finally {
      setLoading(false);
    }
  }

  // Compact menus sit at the left of a bottom bar, so they open towards the right
  const posClass = side === "up"
    ? `bottom-full mb-1.5 ${compact ? "left-0" : "right-0"}`
    : "top-full mt-1.5 right-0";

  const showAllOption = onFetchAll && totalRows && totalRows > rows.length;

  return (
    <div ref={ref} className="relative">
      <Tooltip label={t("ui.exportMenu.export")} side={side === "up" ? "top" : "bottom"}>
        <Button variant="ghost-muted" size={compact ? "icon-xs" : "icon-sm"} data-active={open} aria-expanded={open} onClick={() => setOpen((v) => !v)} aria-label={t("ui.exportMenu.exportData")}>
          {loading ? <Spinner /> : <Download />}
        </Button>
      </Tooltip>

      {open && (
        <div className={`absolute ${posClass} z-50 min-w-44 p-1 bg-popover text-popover-foreground border border-border rounded-lg shadow-lg overflow-hidden`}>
          {showAllOption && (
            <div className="px-2 pt-1.5 pb-1">
              <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                {t("ui.exportMenu.currentPage")}
              </span>
            </div>
          )}
          {formats.map((f) => (
            <button
              key={f.value}
              onClick={() => handleExport(f.value)}
              className="w-full text-left px-2 py-1.5 rounded-md text-sm text-foreground hover:bg-accent cursor-pointer"
            >
              {f.label}
            </button>
          ))}

          {showAllOption && (
            <>
              <div className="-mx-1 my-1 border-t border-border" />
              <div className="px-2 pt-1.5 pb-1">
                <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                  {t(totalRowsEstimated ? "ui.exportMenu.allRowsEstimated" : "ui.exportMenu.allRows", { count: totalRows })}
                </span>
              </div>
              {formats.map((f) => (
                <button
                  key={`all-${f.value}`}
                  onClick={() => handleExportAll(f.value)}
                  disabled={loading}
                  className="w-full text-left px-2 py-1.5 rounded-md text-sm text-foreground hover:bg-accent cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {f.label}
                </button>
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
