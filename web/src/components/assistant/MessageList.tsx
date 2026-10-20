import { useEffect, useRef, type ReactNode } from "react";
import type { Conversation } from "../../lib/assistantConversation";
import { stripContext } from "../../lib/assistantContext";
import type { Decision } from "../../types/assistant";
import { TriangleAlert } from "lucide-react";
import { groupItems, isWorking } from "../../lib/assistantTurns";
import { Marker, MarkerContent, MarkerIcon } from "../chat/Marker";
import Spinner from "../ui/Spinner";
import MarkdownView from "./MarkdownView";
import StepsView from "./Steps";
import { useT } from "../../i18n";

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
  const t = useT();
  const scroller = useRef<HTMLDivElement>(null);
  const blocks = groupItems(conversation.items, conversation.running);
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
      {blocks.map((block) => {
        if (block.kind === "steps") {
          const last = block.index + block.items.length >= conversation.items.length;
          return <StepsView key={`steps-${block.index}`} block={block} working={conversation.running && last} onDecide={onDecide} />;
        }
        const { item, index } = block;
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
          case "error":
            return (
              <Marker key={index} live className="select-text items-start rounded-lg bg-destructive/10 px-3 py-2 text-[13px] text-destructive-foreground">
                <MarkerIcon className="mt-0.5">
                  <TriangleAlert />
                </MarkerIcon>
                <MarkerContent>{item.text}</MarkerContent>
              </Marker>
            );
          case "note":
            return (
              <Marker key={index} variant="separator">
                {item.text}
              </Marker>
            );
          default:
            return null;
        }
      })}
      {isWorking(conversation.items, conversation.running) && (
        <Marker live>
          <MarkerIcon>
            <Spinner />
          </MarkerIcon>
          <MarkerContent shimmer>{t("assistant.messageList.thinking")}</MarkerContent>
        </Marker>
      )}
    </div>
  );
}
