import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import ts from "typescript";

export interface Loose {
  line: number;
  text: string;
}

/** JSX attributes whose value a person reads. */
const TEXT_ATTRIBUTES = new Set(["placeholder", "aria-label", "alt", "label", "description", "title", "hint"]);
const HAS_LETTER = /\p{L}/u;

/** Text in a .tsx source that doesn't go through `t()`: JSX text, text attributes, and strings in expressions that are children. */
export function looseText(source: string, fileName = "file.tsx"): Loose[] {
  const file = ts.createSourceFile(fileName, source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TSX);
  const found: Loose[] = [];
  const add = (node: ts.Node, text: string) => {
    if (HAS_LETTER.test(text)) found.push({ line: ts.getLineAndCharacterOfPosition(file, node.getStart(file)).line + 1, text });
  };

  // Strings under an expression; `t("key")` is how text is meant to get here, so its argument is not text
  function strings(node: ts.Node) {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "t") return;
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) {
      add(node, node.text);
      return;
    }
    if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node)) {
      visit(node);
      return;
    }
    ts.forEachChild(node, strings);
  }

  function visit(node: ts.Node) {
    if (ts.isJsxText(node)) {
      add(node, node.text.replace(/\s+/g, " ").trim());
      return;
    }
    if (ts.isJsxAttribute(node)) {
      if (TEXT_ATTRIBUTES.has(node.name.getText(file)) && node.initializer) strings(node.initializer);
      else if (node.initializer && ts.isJsxExpression(node.initializer) && node.initializer.expression) visitInside(node.initializer.expression);
      return;
    }
    if (ts.isJsxExpression(node) && (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))) {
      if (node.expression) strings(node.expression);
      return;
    }
    ts.forEachChild(node, visit);
  }

  // Elements passed as props (`icon={<X>Text</X>}`) are scanned for their own text
  function visitInside(node: ts.Node) {
    if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node)) visit(node);
    else ts.forEachChild(node, visitInside);
  }

  visit(file);
  return found;
}

/** Every .tsx file under a folder, by path relative to `root`, sorted. */
export function tsxFiles(root: string, folder = "src"): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
      const path = `${dir}/${entry.name}`;
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith(".tsx")) out.push(path);
    }
  };
  walk(folder);
  return out.sort();
}

/** The files that still have loose text, with what they have. */
export function filesWithLooseText(root: string): Map<string, Loose[]> {
  const out = new Map<string, Loose[]>();
  for (const path of tsxFiles(root)) {
    const found = looseText(readFileSync(join(root, path), "utf8"), path);
    if (found.length > 0) out.set(path, found);
  }
  return out;
}
