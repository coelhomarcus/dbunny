import { parseReleaseMarkdown, type Inline } from "../lib/releaseMarkdown";
import { openExternal } from "../lib/openExternal";

/** Release notes, drawn from their Markdown. */
export default function ReleaseNotes({ markdown }: { markdown: string }) {
  const blocks = parseReleaseMarkdown(markdown);
  if (blocks.length === 0) {
    return <p className="text-sm text-muted-foreground">These release notes are empty.</p>;
  }

  return (
    <div className="select-text space-y-3 text-[13px] leading-relaxed">
      {blocks.map((block, index) => {
        if (block.kind === "heading") {
          const Tag = block.level <= 1 ? "h3" : "h4";
          return (
            <Tag key={index} className={index === 0 ? "text-sm font-semibold" : "pt-1 text-sm font-semibold"}>
              <Inlines parts={block.inlines} />
            </Tag>
          );
        }
        if (block.kind === "list") {
          const List = block.ordered ? "ol" : "ul";
          return (
            <List key={index} className={block.ordered ? "list-decimal space-y-1.5 pl-5" : "list-disc space-y-1.5 pl-4"}>
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
            <pre key={index} className="overflow-x-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed">
              {block.text}
            </pre>
          );
        }
        if (block.kind === "table") {
          return (
            <div key={index} className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full border-collapse text-left text-xs">
                <thead className="bg-muted">
                  <tr>
                    {block.header.map((cell, cellIndex) => (
                      <th key={cellIndex} className="px-2.5 py-1.5 font-medium">
                        <Inlines parts={cell} />
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, rowIndex) => (
                    <tr key={rowIndex} className="border-t border-border">
                      {row.map((cell, cellIndex) => (
                        <td key={cellIndex} className="px-2.5 py-1.5 align-top">
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
        <button
          key={index}
          type="button"
          className="cursor-pointer text-primary underline underline-offset-2"
          onClick={() => openExternal(part.href)}
        >
          {part.text}
        </button>
      );
    }
    return <span key={index}>{part.text}</span>;
  });
}
