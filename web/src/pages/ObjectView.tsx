import { useEffect, useState } from "react";
import { useParams } from "react-router";
import { Check, Copy, ListOrdered, Shapes } from "lucide-react";
import type { ObjectDefinition } from "@/types";
import { DetailSkeleton } from "../components/Skeleton";
import { useApi } from "../contexts/useConnection";
import SqlViewer from "../components/SqlViewer";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";

/** A sequence or a type: a few facts and its DDL. */
export default function ObjectView({ kind }: { kind: "sequence" | "type" }) {
  const { schema, name } = useParams<{ schema: string; name: string }>();
  const api = useApi();
  const key = `${kind}:${schema}.${name}`;
  const [result, setResult] = useState<{ key: string; definition?: ObjectDefinition; error?: string } | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!schema || !name) return;
    let stale = false;
    const k = `${kind}:${schema}.${name}`;
    api
      .getObjectDefinition(schema, name, kind)
      .then((definition) => !stale && setResult({ key: k, definition }))
      .catch((e) => !stale && setResult({ key: k, error: typeof e === "string" ? e : String(e) }));
    return () => {
      stale = true;
    };
  }, [schema, name, kind, api]);

  const loading = !result || result.key !== key;
  const definition = loading ? undefined : result.definition;
  const label = kind === "sequence" ? "Sequence" : `Type${definition ? ` (${definition.kind})` : ""}`;

  return (
    <div className="h-full flex flex-col">
      <PageHeader>
        <HeaderTitle
          label={`${label} · ${schema}`}
          icon={kind === "sequence" ? <ListOrdered className="text-teal-500" /> : <Shapes className="text-pink-500" />}
        >
          {name}
        </HeaderTitle>
      </PageHeader>

      {loading ? (
        <DetailSkeleton />
      ) : !definition ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-destructive-foreground">{result.error}</div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="shrink-0 border-b border-border px-4 py-4">
            <div className="grid max-w-xl grid-cols-2 gap-x-8 gap-y-2.5 text-sm">
              {definition.facts.map(([factLabel, value]) => (
                <div key={factLabel} className="flex items-baseline gap-2">
                  <span className="shrink-0 text-muted-foreground">{factLabel}</span>
                  <span className="truncate font-mono text-foreground">{value}</span>
                </div>
              ))}
            </div>
            {definition.comment && <p className="mt-3 text-sm text-muted-foreground">{definition.comment}</p>}
          </div>
          <div className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-muted px-4">
            <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">Definition</span>
            <button
              onClick={async () => {
                await navigator.clipboard.writeText(definition.ddl);
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
              }}
              className="flex h-6 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground"
            >
              {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <div className="min-h-0 flex-1">
            <SqlViewer value={definition.ddl} />
          </div>
        </div>
      )}
    </div>
  );
}
