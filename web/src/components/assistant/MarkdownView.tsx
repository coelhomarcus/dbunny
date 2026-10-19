import type { ReactNode } from "react";
import { parseMarkdown, type Inline } from "../../lib/markdown";
import { openExternal } from "../../lib/openExternal";

/** The assistant's Markdown, drawn; `renderCode` draws the fenced code blocks of its own. */
export default function MarkdownView({
  source,
  renderCode,
}: {
  source: string;
  renderCode?: (language: string, text: string) => ReactNode;
}) {
  const blocks = parseMarkdown(source);
  return (
    <div className="select-text space-y-2.5 text-[13px] leading-relaxed">
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
            <List key={index} className={block.ordered ? "list-decimal space-y-1 pl-5" : "list-disc space-y-1 pl-4"}>
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
            <div key={index} className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full border-collapse text-left text-xs">
                <thead className="bg-card-header">
                  <tr>
                    {block.header.map((cell, cellIndex) => (
                      <th key={cellIndex} className="px-2 py-1 font-medium">
                        <Inlines parts={cell} />
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, rowIndex) => (
                    <tr key={rowIndex} className="border-t border-border">
                      {row.map((cell, cellIndex) => (
                        <td key={cellIndex} className="px-2 py-1 align-top">
                          <Inlines parts={cell} />
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
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
        <code key={index} className="rounded bg-muted px-1 py-0.5 font-mono text-[12px]">
          {part.text}
        </code>
      );
    }
    if (part.kind === "strong") return <strong key={index}>{part.text}</strong>;
    if (part.kind === "link") {
      return (
        <button key={index} type="button" className="cursor-pointer text-primary underline underline-offset-2" onClick={() => openExternal(part.href)}>
          {part.text}
        </button>
      );
    }
    return <span key={index}>{part.text}</span>;
  });
}
