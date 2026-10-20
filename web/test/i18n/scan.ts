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
/** Props that hold a name, a path or a style, not words for a person. */
const NON_TEXT_ATTRIBUTES = new Set(["className", "class", "key", "id", "type", "role", "href", "src", "side", "variant", "size", "tone", "name", "value", "to", "htmlFor", "style", "rel", "target", "align", "mode", "kind", "d", "viewBox", "fill", "stroke", "transform", "points", "width", "height", "autoComplete", "inputMode", "spellCheck", "tabIndex", "lang", "dir", "xmlns", "ref", "list", "accept", "direction", "orientation", "icon", "keys"]);
/** Object properties that hold words for a person: `{ label: "Connection" }`. */
const TEXT_PROPERTIES = new Set(["label", "title", "description", "hint", "placeholder", "text", "message", "heading", "body", "confirmLabel", "cancelLabel", "subtitle", "caption", "tooltip", "emptyText"]);
/** A capitalized word or sentence: "Close", "Discard and close". */
const LOOKS_LIKE_TEXT = /^\p{Lu}\p{Ll}/u;
/** Names that are never translated. */
const BRANDS = ["English", "Português (Brasil)", "DBunny", "PostgreSQL", "Postgres", "MySQL", "MariaDB", "SQLite", "SQL Server", "Claude Code", "Claude", "Codex", "ChatGPT", "Anthropic", "OpenAI", "GitHub"];
/** Elements whose text is code or a key, not language. */
const TECHNICAL_TAGS = new Set(["code", "kbd", "pre", "samp"]);

/** Keywords and acronyms (no lowercase letter) and brand names say the same in every language. */
function isTechnical(text: string): boolean {
  if (/^#[0-9a-fA-F]{3,8}$/.test(text)) return true;
  if (/^(ms|s|min|h|d|B|kB|MB|GB|TB|px|%)$/.test(text.trim())) return true;
  // camelCase, snake_case and kebab-case names are identifiers
  if (/^[a-z][a-z0-9]*([A-Z_-][a-z0-9]+)+$/.test(text)) return true;
  let rest = text;
  for (const brand of BRANDS) rest = rest.split(brand).join(" ");
  return !/\p{Ll}/u.test(rest);
}

/** Text in a .tsx source that doesn't go through `t()`: JSX text, text attributes, and strings in expressions that are children. */
export function looseText(source: string, fileName = "file.tsx"): Loose[] {
  const file = ts.createSourceFile(fileName, source, ts.ScriptTarget.ES2022, true, fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const found: Loose[] = [];
  const inTechnicalTag = (node: ts.Node) => {
    for (let up = node.parent; up; up = up.parent) {
      if (ts.isJsxElement(up) && TECHNICAL_TAGS.has(up.openingElement.tagName.getText(file))) return true;
    }
    return false;
  };
  const lines = source.split("\n");
  const add = (node: ts.Node, text: string) => {
    // `i18n-ignore` on a line says its words are not for the person (said to the agent, a name in the database)
    const line = ts.getLineAndCharacterOfPosition(file, node.getStart(file)).line;
    if (lines[line]?.includes("i18n-ignore")) return;
    if (HAS_LETTER.test(text) && !isTechnical(text) && !inTechnicalTag(node)) found.push({ line: ts.getLineAndCharacterOfPosition(file, node.getStart(file)).line + 1, text });
  };

  // Text an expression hands to the screen: the value itself, the branches of a condition, what a
  // callback returns. Arguments of calls, comparisons and statements are logic, not text.
  function strings(node: ts.Node | undefined): void {
    if (!node) return;
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return add(node, node.text);
    if (ts.isTemplateExpression(node)) {
      add(node.head, node.head.text);
      for (const span of node.templateSpans) {
        strings(span.expression);
        add(span.literal, span.literal.text);
      }
      return;
    }
    if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node)) return visit(node);
    if (ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isNonNullExpression(node)) return strings(node.expression);
    if (ts.isConditionalExpression(node)) {
      strings(node.whenTrue);
      strings(node.whenFalse);
      return;
    }
    if (ts.isBinaryExpression(node)) {
      const op = node.operatorToken.kind;
      if (op === ts.SyntaxKind.AmpersandAmpersandToken) return strings(node.right);
      if (op === ts.SyntaxKind.BarBarToken || op === ts.SyntaxKind.QuestionQuestionToken) {
        strings(node.left);
        strings(node.right);
      }
      return;
    }
    if (ts.isArrayLiteralExpression(node)) return node.elements.forEach(strings);
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) return ts.isBlock(node.body) ? returns(node.body) : strings(node.body);
    if (ts.isCallExpression(node)) {
      if (ts.isIdentifier(node.expression) && node.expression.text === "t") return;
      // `["a", "b"].map(...)` shows the list; a callback shows what it returns
      if (ts.isPropertyAccessExpression(node.expression)) strings(node.expression.expression);
      node.arguments.forEach((argument) => (ts.isArrowFunction(argument) || ts.isFunctionExpression(argument) ? strings(argument) : visitInside(argument)));
    }
  }

  // The expressions a function body returns, not the ones it only computes with
  function returns(node: ts.Node): void {
    if (ts.isReturnStatement(node)) return strings(node.expression);
    if (ts.isFunctionLike(node) || ts.isClassLike(node)) return;
    ts.forEachChild(node, returns);
  }

  function visit(node: ts.Node) {
    if (ts.isJsxText(node)) {
      add(node, node.text.replace(/\s+/g, " ").trim());
      return;
    }
    if (ts.isJsxAttribute(node)) {
      const attribute = node.name.getText(file);
      if (!TEXT_ATTRIBUTES.has(attribute) && !NON_TEXT_ATTRIBUTES.has(attribute) && !attribute.startsWith("data-") && !/^on[A-Z]/.test(attribute) && node.initializer && ts.isStringLiteral(node.initializer) && LOOKS_LIKE_TEXT.test(node.initializer.text)) {
        add(node.initializer, node.initializer.text);
      } else if (TEXT_ATTRIBUTES.has(attribute) && node.initializer) strings(ts.isJsxExpression(node.initializer) ? node.initializer.expression : node.initializer);
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

  // Words in data: the labels of tabs, the options of a menu, the text of a notice
  function properties(node: ts.Node) {
    if (ts.isPropertyAssignment(node) && (ts.isIdentifier(node.name) || ts.isStringLiteral(node.name)) && TEXT_PROPERTIES.has(node.name.text)) strings(node.initializer);
    ts.forEachChild(node, properties);
  }

  // A capitalized phrase of two words or more is a sentence, wherever the code keeps it
  const SENTENCE = /^\p{Lu}[\p{Ll}'’]+( [\p{L}'’.,;:!?()-]+)+/u;
  function sentences(node: ts.Node) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) return;
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "t") return;
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && ts.isIdentifier(node.expression.expression) && node.expression.expression.text === "console") return;
    if (ts.isNewExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "Error") return;
    if (ts.isBinaryExpression(node) && (node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken || node.operatorToken.kind === ts.SyntaxKind.ExclamationEqualsEqualsToken)) return;
    if (ts.isJsxAttribute(node) && NON_TEXT_ATTRIBUTES.has(node.name.getText(file))) return;
    if (ts.isCaseClause(node)) return node.statements.forEach(sentences);
    // `top["Execution Time"]` reads a value by its name
    if (ts.isElementAccessExpression(node)) return sentences(node.expression);
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) {
      if (SENTENCE.test(node.text)) add(node, node.text);
      return;
    }
    ts.forEachChild(node, sentences);
  }

  visit(file);
  properties(file);
  sentences(file);
  // The same text can be reached by both walks
  return found.filter((hit, index) => found.findIndex((other) => other.line === hit.line && other.text === hit.text) === index);
}

/** Every source file under a folder that can hold interface text, by path relative to `root`, sorted. */
export function sourceFiles(root: string, folder = "src"): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
      const path = `${dir}/${entry.name}`;
      if (entry.isDirectory()) walk(path);
      else if (/\.tsx?$/.test(entry.name) && !entry.name.endsWith(".d.ts") && !path.startsWith("src/i18n/locales") && !path.startsWith("src/types")) out.push(path);
    }
  };
  walk(folder);
  return out.sort();
}

/** The files that still have loose text, with what they have. */
export function filesWithLooseText(root: string): Map<string, Loose[]> {
  const out = new Map<string, Loose[]>();
  for (const path of sourceFiles(root)) {
    const found = looseText(readFileSync(join(root, path), "utf8"), path);
    if (found.length > 0) out.set(path, found);
  }
  return out;
}
