import { useEffect, useState } from "react";
import { Braces, Copy, Check } from "lucide-react";
import { DetailSkeleton } from "../components/Skeleton";
import { useApi } from "../contexts/useConnection";
import SqlViewer from "../components/SqlViewer";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import type { FunctionDetail } from "@/types";
import { writeClipboardText } from "../lib/clipboard";
import { useNotify } from "../contexts/NoticeProvider";

export default function FunctionView({ schema, name, detail: args }: { schema: string; name: string; detail: string }) {
  const [detail, setDetail] = useState<FunctionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const notify = useNotify();
  const [copied, setCopied] = useState(false);
  const api = useApi();

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
        if (stale) return;
        const text = typeof e === "string" ? e : String(e);
        notify({ tone: "error", text });
        setError(text);
        setLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [schema, name, args, api, notify]);

  async function copySource() {
    if (!detail?.source) return;
    await writeClipboardText(detail.source);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="h-full flex flex-col">
      <PageHeader>
        <HeaderTitle label={`Function · ${schema}`} icon={<Braces className="text-purple-500" />}>
          {name}
        </HeaderTitle>
      </PageHeader>

      {loading ? (
        <DetailSkeleton />
      ) : error ? (
        <div className="flex-1 flex items-center justify-center p-4">
          <span className="text-sm text-muted-foreground">Couldn't read this function.</span>
        </div>
      ) : detail ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* Properties */}
          <div className="px-4 py-4 border-b border-border shrink-0">
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
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            <div className="flex h-9 shrink-0 items-center justify-between px-4 bg-muted border-b border-border">
              <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                Definition
              </span>
              <button
                onClick={copySource}
                className="flex items-center gap-1.5 h-6 px-2 text-xs text-muted-foreground hover:text-foreground hover:bg-accent rounded-md transition-colors cursor-pointer"
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
            <div className="flex-1 min-h-0">
              <SqlViewer value={detail.source} />
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Property({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-muted-foreground shrink-0">{label}</span>
      <span className="select-text font-mono text-foreground">{value}</span>
    </div>
  );
}
