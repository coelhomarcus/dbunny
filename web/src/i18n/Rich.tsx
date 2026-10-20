import { Fragment, type ReactNode } from "react";
import { useLocale } from "./index.ts";
import { raw } from "./store.ts";
import { interpolate, parseRich } from "./translate.ts";
import type { en } from "./locales/en.ts";
import type { MessageKey, Params, RichNode } from "./messages.ts";

type Tags = Record<string, (children: ReactNode) => ReactNode>;

function render(nodes: RichNode[], tags: Tags, params?: Params): ReactNode {
  return nodes.map((node, index) => {
    // The values go into the text after the tags are read, so a value can't add tags of its own
    if (typeof node === "string") return <Fragment key={index}>{interpolate(node, params)}</Fragment>;
    const wrap = tags[node.tag];
    const inner = render(node.children, tags, params);
    return <Fragment key={index}>{wrap ? wrap(inner) : inner}</Fragment>;
  });
}

/** A message with elements inside: `<link>docs</link>` becomes whatever `tags.link` makes of "docs". */
export function Rich({ id, params, tags = {} }: { id: MessageKey<typeof en>; params?: Params; tags?: Tags }) {
  useLocale();
  const count = typeof params?.count === "number" ? params.count : undefined;
  return <>{render(parseRich(raw(id, count)), tags, params)}</>;
}
