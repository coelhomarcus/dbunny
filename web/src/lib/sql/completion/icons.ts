import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  Braces, Columns3, Database, Eye, Hash, KeyRound, Layers, Link2, Shapes, Sigma, SquareFunction, SquareTerminal, Table2, Variable,
  type LucideIcon,
} from "lucide-react";
import type { StaticKey } from "../../../i18n/store.ts";

/** What a suggestion is; it is the `type` of each completion and picks its icon. */
export const ITEM_KINDS = [
  "schema", "table", "view", "materializedView", "column", "columnPrimaryKey", "columnForeignKey",
  "function", "procedure", "builtinFunction", "keyword", "dataType", "snippet", "local",
] as const;
export type ItemKind = (typeof ITEM_KINDS)[number];

const ICONS: Record<ItemKind, LucideIcon> = {
  schema: Database,
  table: Table2,
  view: Eye,
  materializedView: Layers,
  column: Columns3,
  columnPrimaryKey: KeyRound,
  columnForeignKey: Link2,
  function: SquareFunction,
  procedure: SquareTerminal,
  builtinFunction: Sigma,
  keyword: Hash,
  dataType: Shapes,
  snippet: Braces,
  local: Variable,
};

/** The name of each kind, said next to the icon so the icon isn't the only clue. */
export const ICON_LABEL: Record<ItemKind, StaticKey> = {
  schema: "ui.sqlCompletion.schema",
  table: "ui.sqlCompletion.table",
  view: "ui.sqlCompletion.view",
  materializedView: "ui.sqlCompletion.materializedView",
  column: "ui.sqlCompletion.column",
  columnPrimaryKey: "ui.sqlCompletion.columnPrimaryKey",
  columnForeignKey: "ui.sqlCompletion.columnForeignKey",
  function: "ui.sqlCompletion.function",
  procedure: "ui.sqlCompletion.procedure",
  builtinFunction: "ui.sqlCompletion.builtinFunction",
  keyword: "ui.sqlCompletion.keyword",
  dataType: "ui.sqlCompletion.dataType",
  snippet: "ui.sqlCompletion.snippet",
  local: "ui.sqlCompletion.local",
};

const markup = new Map<ItemKind, string>();

/** The kind's icon as SVG markup, drawn once. */
export function iconSvg(kind: ItemKind): string {
  let svg = markup.get(kind);
  if (!svg) {
    svg = renderToStaticMarkup(createElement(ICONS[kind], { size: 14, strokeWidth: 2, "aria-hidden": true }));
    markup.set(kind, svg);
  }
  return svg;
}

export function isItemKind(type: string | undefined): type is ItemKind {
  return (ITEM_KINDS as readonly string[]).includes(type ?? "");
}
