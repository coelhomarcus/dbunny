import { useState, useRef, useEffect } from "react";
import { Download, Loader } from "lucide-react";
import { exportData, type ExportFormat } from "../lib/exportData";
import type { QueryColumn } from "@/types";

interface ExportMenuProps {
  columns: QueryColumn[];
  rows: unknown[][];
  defaultName: string;
  side?: "up" | "down";
  totalRows?: number;
  onFetchAll?: () => Promise<{ columns: QueryColumn[]; rows: unknown[][] }>;
}

const formats: { value: ExportFormat; label: string }[] = [
  { value: "csv", label: "CSV" },
  { value: "json", label: "JSON" },
];

export default function ExportMenu({
  columns, rows, defaultName,
  side = "down", totalRows, onFetchAll,
}: ExportMenuProps) {
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

  const posClass = side === "up"
    ? "bottom-full mb-1.5 right-0"
    : "top-full mt-1.5 right-0";

  const showAllOption = onFetchAll && totalRows && totalRows > rows.length;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 px-2.5 py-1.5 text-zinc-400 hover:text-zinc-200 text-sm rounded-lg transition-colors cursor-pointer"
        title="Export data"
      >
        {loading ? <Loader size={14} className="animate-spin" /> : <Download size={14} />}
      </button>

      {open && (
        <div className={`absolute ${posClass} z-50 min-w-40 bg-zinc-800 border border-zinc-700/60 rounded-lg shadow-xl overflow-hidden`}>
          {showAllOption && (
            <div className="px-3 pt-2 pb-1">
              <span className="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">
                Current page
              </span>
            </div>
          )}
          {formats.map((f) => (
            <button
              key={f.value}
              onClick={() => handleExport(f.value)}
              className="w-full text-left px-3 py-2 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-white transition-colors cursor-pointer"
            >
              {f.label}
            </button>
          ))}

          {showAllOption && (
            <>
              <div className="border-t border-zinc-700/40" />
              <div className="px-3 pt-2 pb-1">
                <span className="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">
                  All rows ({totalRows.toLocaleString()})
                </span>
              </div>
              {formats.map((f) => (
                <button
                  key={`all-${f.value}`}
                  onClick={() => handleExportAll(f.value)}
                  disabled={loading}
                  className="w-full text-left px-3 py-2 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-white transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
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
