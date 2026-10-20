import { useEffect, useRef, type ReactNode } from "react";
import type { Conversation } from "../../lib/assistantConversation";
import { stripContext } from "../../lib/assistantContext";
import type { Decision } from "../../types/assistant";
import MarkdownView from "./MarkdownView";
import ToolCard from "./ToolCard";
import SqlSnippet from "./SqlSnippet";
import ApprovalButtons from "./ApprovalButtons";

/** What was said in the conversation, and what the assistant did meanwhile. Follows the end while the person stays there. */
export default function MessageList({
  conversation,
  onDecide,
  renderCode,
}: {
  conversation: Conversation;
  onDecide: (approvalId: string, decision: Decision) => void;
  renderCode?: (language: string, text: string) => ReactNode;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);

  useEffect(() => {
    const el = scroller.current;
    if (el && atEnd.current) el.scrollTop = el.scrollHeight;
  }, [conversation]);

  return (
    <div
      ref={scroller}
      className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3"
      onScroll={(event) => {
        const el = event.currentTarget;
        atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
      }}
    >
      {conversation.items.map((item, index) => {
        switch (item.kind) {
          case "user": {
            // The database context some agents get in the first message is not for the person
            const said = stripContext(item.text);
            return (
              <div key={index} className="flex justify-end">
                {said.includes("```") ? (
                  // A request from the editor carries its statement in a fence
                  <div className="max-w-[90%] rounded-2xl rounded-br-md bg-muted px-3 py-2">
                    <MarkdownView source={said} />
                  </div>
                ) : (
                  <p className="max-w-[85%] select-text whitespace-pre-wrap rounded-2xl rounded-br-md bg-muted px-3 py-2 text-[13px] leading-relaxed">{said}</p>
                )}
              </div>
            );
          }
          case "agent":
            return <MarkdownView key={index} source={item.text} renderCode={renderCode} />;
          case "thought":
            return (
              <details key={index} className="text-xs text-muted-foreground">
                <summary className="cursor-pointer select-none">Thinking…</summary>
                <p className="mt-1 select-text whitespace-pre-wrap">{item.text}</p>
              </details>
            );
          case "tool":
            return <ToolCard key={item.id} item={item} onDecide={onDecide} />;
          case "approval":
            return (
              <div key={item.approval.id} className="overflow-hidden rounded-lg border border-border bg-card text-xs">
                <div className="px-2.5 py-1.5 font-medium">{item.approval.analyze ? "Run a query to explain it" : "Run a query"}</div>
                <div className="border-t border-border">
                  <SqlSnippet sql={item.approval.sql} />
                </div>
                <div className="border-t border-border px-2.5 py-2">
                  <ApprovalButtons onDecide={(decision) => onDecide(item.approval.id, decision)} />
                </div>
              </div>
            );
          case "error":
            return (
              <p key={index} className="select-text rounded-lg bg-destructive/10 px-3 py-2 text-[13px] text-destructive-foreground">
                {item.text}
              </p>
            );
        }
      })}
    </div>
  );
}
