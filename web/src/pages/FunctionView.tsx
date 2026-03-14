import { useEffect, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { Loader, Braces, Copy, Check } from "lucide-react";
import { api } from "../lib/api";
import type { FunctionDetail } from "@/types";

export default function FunctionView() {
  const { schema, name } = useParams<{ schema: string; name: string }>();
  const [searchParams] = useSearchParams();
  const args = searchParams.get("args") ?? "";
  const [detail, setDetail] = useState<FunctionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!schema || !name) return;
    let stale = false;
    setLoading(true);
    setError(null);
    api
      .getFunctionDetail(schema, name, args)
      .then((d) => {
        if (!stale) {
          setDetail(d);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!stale) {
          setError(String(e));
          setLoading(false);
        }
      });
    return () => {
      stale = true;
    };
  }, [schema, name, args]);

  async function copySource() {
    if (!detail?.source) return;
    await navigator.clipboard.writeText(detail.source);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="h-full flex flex-col gap-2">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-zinc-900 border border-zinc-800/60 rounded-xl shrink-0">
        <div className="flex items-center gap-2">
          <Braces size={16} className="text-purple-400" />
          <h2 className="text-base font-medium">
            <span className="text-zinc-500">{schema}.</span>
            {name}
          </h2>
        </div>
      </div>

      {loading ? (
        <div className="flex-1 flex items-end justify-end p-4">
          <Loader size={16} className="animate-spin text-zinc-500" />
        </div>
      ) : error ? (
        <div className="flex-1 flex items-center justify-center p-4">
          <span className="text-sm text-red-400">{error}</span>
        </div>
      ) : detail ? (
        <div className="flex-1 overflow-auto flex flex-col gap-3">
          {/* Properties */}
          <div className="px-5 py-4 bg-zinc-900 border border-zinc-800/60 rounded-xl shrink-0">
            <div className="grid grid-cols-2 gap-x-8 gap-y-2.5 text-sm max-w-xl">
              <Property label="Type" value={detail.kind} />
              <Property label="Language" value={detail.language} />
              <Property label="Returns" value={detail.returnType} />
              <Property
                label="Arguments"
                value={detail.argumentTypes || "(none)"}
              />
              <Property label="Volatility" value={detail.volatility} />
              <Property
                label="Strict"
                value={detail.isStrict ? "YES" : "NO"}
              />
              <Property label="Owner" value={detail.owner} />
              {detail.description && (
                <div className="col-span-2">
                  <Property label="Description" value={detail.description} />
                </div>
              )}
            </div>
          </div>

          {/* Source code */}
          <div className="flex-1 min-h-0 flex flex-col bg-zinc-900 border border-zinc-800/60 rounded-xl overflow-hidden">
            <div className="flex items-center justify-between px-5 py-3 bg-zinc-800/40 border-b border-zinc-800/40">
              <span className="text-sm font-medium text-zinc-400">
                Definition
              </span>
              <button
                onClick={copySource}
                className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-zinc-500 hover:text-zinc-300 hover:bg-zinc-700/50 rounded-lg transition-colors"
              >
                {copied ? (
                  <>
                    <Check size={13} /> Copied
                  </>
                ) : (
                  <>
                    <Copy size={13} /> Copy
                  </>
                )}
              </button>
            </div>
            <pre className="flex-1 p-5 text-sm text-zinc-300 font-mono overflow-auto whitespace-pre leading-relaxed">
              {detail.source}
            </pre>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Property({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-zinc-500 shrink-0">{label}</span>
      <span className="text-zinc-200 font-mono">{value}</span>
    </div>
  );
}
