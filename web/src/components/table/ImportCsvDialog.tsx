import { useEffect, useMemo, useState } from "react";
import { FileUp, FolderOpen, Loader } from "lucide-react";
import type { ColumnInfo } from "@/types";
import { detectDelimiter, looksLikeHeader, matchColumns, parseCsv } from "../../lib/csv";
import { cn } from "../../lib/cn";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Checkbox from "../Checkbox";
import { selectClass } from "../ui/styles";

const DELIMITERS = [
  { value: ",", label: "Comma" },
  { value: ";", label: "Semicolon" },
  { value: "\t", label: "Tab" },
  { value: "|", label: "Pipe" },
];
const PREVIEW_ROWS = 5;

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <div className="flex cursor-pointer items-center gap-2 select-none text-[13px]" onClick={() => onChange(!checked)}>
      <Checkbox checked={checked} onChange={() => onChange(!checked)} />
      {label}
    </div>
  );
}

function ImportForm({
  schema,
  table,
  columns,
  initialText,
  onImport,
  onClose,
}: {
  schema: string;
  table: string;
  columns: ColumnInfo[];
  /** Pasted data to start from, instead of choosing a file. */
  initialText?: string;
  onImport: (columns: string[], rows: (string | null)[][]) => Promise<number>;
  onClose: () => void;
}) {
  const writable = columns.filter((c) => !c.isGenerated).map((c) => c.name);
  // Pasted data starts parsed: delimiter, header and column mapping are guessed from it
  const initial = (() => {
    if (!initialText) return null;
    const delimiter = detectDelimiter(initialText);
    const rows = parseCsv(initialText, delimiter);
    const header = looksLikeHeader(rows[0] ?? [], writable);
    const first = rows[0] ?? [];
    return {
      delimiter,
      header,
      mapping: header ? matchColumns(first, writable) : first.map((_, i) => writable[i] ?? null),
    };
  })();
  const [file, setFile] = useState<{ name: string; text: string } | null>(initialText ? { name: "Pasted data", text: initialText } : null);
  const [delimiter, setDelimiter] = useState(initial?.delimiter ?? ",");
  const [hasHeader, setHasHeader] = useState(initial?.header ?? true);
  const [emptyAsNull, setEmptyAsNull] = useState(true);
  // Per CSV column: the table column it goes into, or null to skip it
  const [mapping, setMapping] = useState<(string | null)[]>(initial?.mapping ?? []);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const parsed = useMemo(() => (file ? parseCsv(file.text, delimiter) : []), [file, delimiter]);
  const width = parsed.reduce((max, r) => Math.max(max, r.length), 0);
  const headers = hasHeader ? (parsed[0] ?? []) : Array.from({ length: width }, (_, i) => `Column ${i + 1}`);
  const dataRows = hasHeader ? parsed.slice(1) : parsed;
  const used = mapping.filter((m): m is string => m !== null);
  const duplicates = used.filter((m, i) => used.indexOf(m) !== i);

  function remap(nextParsed: string[][], header: boolean) {
    const first = nextParsed[0] ?? [];
    setMapping(header ? matchColumns(first, writable) : first.map((_, i) => writable[i] ?? null));
  }

  async function chooseFile() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { readTextFile } = await import("@tauri-apps/plugin-fs");
    const path = await open({ multiple: false, filters: [{ name: "CSV", extensions: ["csv", "tsv", "txt"] }] });
    if (typeof path !== "string") return;
    setError(null);
    try {
      const text = await readTextFile(path);
      const detected = detectDelimiter(text);
      setFile({ name: path.split(/[\\/]/).pop() ?? path, text });
      setDelimiter(detected);
      remap(parseCsv(text, detected), hasHeader);
    } catch (err) {
      setError(typeof err === "string" ? err : "Couldn't read the file");
    }
  }

  async function runImport() {
    const picked = mapping.map((target, i) => ({ target, i })).filter((m) => m.target !== null);
    const rows = dataRows.map((row) =>
      picked.map(({ i }) => {
        const value = row[i] ?? "";
        return emptyAsNull && value === "" ? null : value;
      }),
    );
    setBusy(true);
    setError(null);
    try {
      await onImport(picked.map((m) => m.target!), rows);
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : "The import failed");
      setBusy(false);
    }
  }

  // Pasting while the dialog is open replaces the data
  useEffect(() => {
    function onPaste(e: ClipboardEvent) {
      const text = e.clipboardData?.getData("text/plain") ?? "";
      if (!text.trim() || (e.target as HTMLElement)?.closest("input, textarea, select")) return;
      e.preventDefault();
      const detected = detectDelimiter(text);
      const rows = parseCsv(text, detected);
      const header = looksLikeHeader(rows[0] ?? [], writable);
      setFile({ name: "Pasted data", text });
      setDelimiter(detected);
      setHasHeader(header);
      remap(rows, header);
    }
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  });

  const canImport = file !== null && used.length > 0 && duplicates.length === 0 && dataRows.length > 0 && !busy;

  return (
    <Dialog
      open
      onClose={onClose}
      className="max-w-3xl"
      icon={<FileUp />}
      title={
        <>
          {initialText ? "Paste rows" : "Import CSV"} <span className="font-normal text-muted-foreground">into {schema}.{table}</span>
        </>
      }
      description="All rows are inserted in one transaction: if one fails, nothing is imported."
      footer={
        <>
          {error ? (
            <p className="mr-auto min-w-0 flex-1 select-text break-words text-xs text-destructive-foreground">{error}</p>
          ) : (
            file && <p className="mr-auto text-xs text-muted-foreground tabular-nums">{dataRows.length.toLocaleString()} rows</p>
          )}
          <Button variant="neutral" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={runImport} disabled={!canImport}>
            {busy && <Loader className="animate-spin" />}
            Import{file && dataRows.length > 0 ? ` ${dataRows.length.toLocaleString()} rows` : ""}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={chooseFile}>
            <FolderOpen />
            {file ? "Choose another file" : "Choose a CSV file"}
          </Button>
          {file && <span className="truncate font-mono text-xs text-muted-foreground">{file.name}</span>}
          {!file && <span className="text-xs text-muted-foreground">or paste rows copied from a spreadsheet (⌘V / Ctrl+V)</span>}
        </div>

        {file && (
          <>
            <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
              <label className="flex items-center gap-2 text-[13px]">
                Delimiter
                <select
                  value={delimiter}
                  onChange={(e) => {
                    setDelimiter(e.target.value);
                    remap(parseCsv(file.text, e.target.value), hasHeader);
                  }}
                  className={cn(selectClass, "h-7")}
                >
                  {DELIMITERS.map((d) => (
                    <option key={d.label} value={d.value}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </label>
              <Toggle
                checked={hasHeader}
                onChange={(v) => {
                  setHasHeader(v);
                  remap(parsed, v);
                }}
                label="First row has column names"
              />
              <Toggle checked={emptyAsNull} onChange={setEmptyAsNull} label="Empty values become NULL" />
            </div>

            <div className="max-h-72 overflow-auto rounded-lg border border-border">
              <table className="w-full text-xs">
                <thead className="sticky top-0 bg-card-header">
                  <tr>
                    {headers.map((h, i) => (
                      <th key={i} className="min-w-36 border-b border-border px-2 py-1.5 text-left align-top font-normal">
                        <div className="truncate font-medium" >{h || <span className="text-muted-foreground">(no name)</span>}</div>
                        <select
                          aria-label={`Table column for ${h || `column ${i + 1}`}`}
                          value={mapping[i] ?? ""}
                          onChange={(e) => setMapping((prev) => headers.map((_, j) => (j === i ? e.target.value || null : prev[j] ?? null)))}
                          className={cn(selectClass, "mt-1 h-6 w-full text-xs", duplicates.includes(mapping[i] ?? "\0") && "border-destructive/60")}
                        >
                          <option value="">Skip</option>
                          {writable.map((c) => (
                            <option key={c} value={c}>
                              → {c}
                            </option>
                          ))}
                        </select>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {dataRows.slice(0, PREVIEW_ROWS).map((row, r) => (
                    <tr key={r} className="border-b border-border last:border-0">
                      {headers.map((_, i) => (
                        <td key={i} className={cn("max-w-48 truncate px-2 py-1 font-mono", mapping[i] === null && "text-muted-foreground/60")}>
                          {row[i] === undefined || (row[i] === "" && emptyAsNull) ? <span className="italic text-muted-foreground/70">NULL</span> : row[i]}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {duplicates.length > 0 && (
              <p className="select-text text-xs text-destructive-foreground">Each table column can take only one CSV column ({[...new Set(duplicates)].join(", ")}).</p>
            )}
            {dataRows.length > PREVIEW_ROWS && (
              <p className="text-xs text-muted-foreground">Showing the first {PREVIEW_ROWS} of {dataRows.length.toLocaleString()} rows.</p>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}

/** Imports a CSV file into a table. Mounted only while open, so it starts empty. */
export default function ImportCsvDialog({ open, ...props }: { open: boolean } & Parameters<typeof ImportForm>[0]) {
  return open ? <ImportForm {...props} /> : null;
}
