import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { parseMarkdown, type Inline } from "../../lib/markdown";
import { isWebLink } from "../../lib/links";
import { openExternal } from "../../lib/openExternal";
import ResultTable from "./ResultTable";

/** The assistant's Markdown, drawn; `renderCode` draws the fenced code blocks of its own. */
export default function MarkdownView({
  source,
  renderCode,
  className,
}: {
  source: string;
  renderCode?: (language: string, text: string) => ReactNode;
  /** Replaces the size and color of the text (the thinking is smaller and quieter). */
  className?: string;
}) {
  const blocks = parseMarkdown(source);
  return (
    <div className={cn("select-text space-y-2.5 leading-relaxed", className ?? "text-[13px]")}>
      {blocks.map((block, index) => {
        if (block.kind === "heading") {
          return (
            <h4 key={index} className="pt-1 text-[13px] font-semibold">
              <Inlines parts={block.inlines} />
            </h4>
          );
        }
        if (block.kind === "list") {
          const List = block.ordered ? "ol" : "ul";
          return (
            <List key={index} className={block.ordered ? "list-decimal space-y-1.5 pl-5 marker:text-muted-foreground" : "list-disc space-y-1.5 pl-5 marker:text-muted-foreground"}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>
                  <Inlines parts={item} />
                </li>
              ))}
            </List>
          );
        }
        if (block.kind === "code") {
          return (
            <div key={index}>
              {renderCode?.(block.language, block.text) ?? (
                <pre className="overflow-x-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed">{block.text}</pre>
              )}
            </div>
          );
        }
        if (block.kind === "table") {
          return (
            <div key={index} className="overflow-hidden rounded-lg border border-border bg-card">
              <ResultTable
                columns={block.header.map((cell, cellIndex) => (
                  <Inlines key={cellIndex} parts={cell} />
                ))}
                rows={block.rows.map((row) => row.map((cell, cellIndex) => <Inlines key={cellIndex} parts={cell} />))}
              />
            </div>
          );
        }
        return (
          <p key={index}>
            <Inlines parts={block.inlines} />
          </p>
        );
      })}
    </div>
  );
}

function Inlines({ parts }: { parts: Inline[] }) {
  return parts.map((part, index) => {
    if (part.kind === "code") {
      return (
        <code key={index} className="rounded-md border border-primary/15 bg-primary/10 px-1.5 py-px font-mono text-[0.92em] font-medium text-primary">
          {part.text}
        </code>
      );
    }
    if (part.kind === "strong") return <strong key={index}>{part.text}</strong>;
    if (part.kind === "link") {
      // Text from the database can end up in a link: only web and mail ones are links
      if (!isWebLink(part.href)) return <span key={index}>{part.text}</span>;
      return (
        <button key={index} type="button" className="cursor-pointer text-primary underline underline-offset-2" onClick={() => openExternal(part.href)}>
          {part.text}
        </button>
      );
    }
    return <span key={index}>{part.text}</span>;
  });
}
