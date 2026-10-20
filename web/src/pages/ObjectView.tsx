import { useEffect, useState } from "react";
import { Check, Copy, ListOrdered, ListTree, Shapes, Zap } from "lucide-react";
import type { ObjectDefinition } from "@/types";
import { DetailSkeleton } from "../components/Skeleton";
import { useApi } from "../contexts/useConnection";
import SqlViewer from "../components/SqlViewer";
import { HeaderTitle, PageHeader } from "../components/layout/PageHeader";
import { writeClipboardText } from "../lib/clipboard";
import { useNotify } from "../contexts/NoticeProvider";
import { useT } from "../i18n";
import { errorText } from "../lib/appError";

/** A sequence, type, index or trigger: a few facts and its DDL. */
export default function ObjectView({
  schema,
  name,
  kind,
}: {
  schema: string;
  name: string;
  kind: "sequence" | "type" | "index" | "trigger";
}) {
  const t = useT();
  const api = useApi();
  const notify = useNotify();
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
      .catch((e) => {
        if (stale) return;
        const text = errorText(e);
        notify({ tone: "error", text });
        setResult({ key: k, error: text });
      });
    return () => {
      stale = true;
    };
  }, [schema, name, kind, api, notify]);

  const loading = !result || result.key !== key;
  const definition = loading ? undefined : result.definition;
  const label =
    kind === "sequence" ? "Sequence" : kind === "index" ? "Index" : kind === "trigger" ? "Trigger" : `Type${definition ? ` (${definition.kind})` : ""}`;
  const icon =
    kind === "sequence" ? (
      <ListOrdered className="text-teal-500" />
    ) : kind === "index" ? (
      <ListTree className="text-orange-500" />
    ) : kind === "trigger" ? (
      <Zap className="text-yellow-500" />
    ) : (
      <Shapes className="text-pink-500" />
    );

  return (
    <div className="h-full flex flex-col">
      <PageHeader>
        <HeaderTitle label={`${label} · ${schema}`} icon={icon}>
          {name}
        </HeaderTitle>
      </PageHeader>

      {loading ? (
        <DetailSkeleton />
      ) : !definition ? (
        <div className="flex flex-1 items-center justify-center p-4 text-sm text-muted-foreground">{t("schema.objectView.couldntReadThisObject")}</div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="shrink-0 border-b border-border px-4 py-4">
            <div className="grid max-w-xl grid-cols-2 gap-x-8 gap-y-2.5 text-sm">
              {definition.facts.map(([factLabel, value]) => (
                <div key={factLabel} className="flex items-baseline gap-2">
                  <span className="shrink-0 text-muted-foreground">{factLabel}</span>
                  <span className="select-text truncate font-mono text-foreground">{value}</span>
                </div>
              ))}
            </div>
            {definition.comment && <p className="mt-3 select-text text-sm text-muted-foreground">{definition.comment}</p>}
          </div>
          <div className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-muted px-4">
            <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">{t("schema.objectView.definition")}</span>
            <button
              onClick={async () => {
                await writeClipboardText(definition.ddl);
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
              }}
              className="flex h-6 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground"
            >
              {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? t("common.copied") : t("common.copy")}
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
