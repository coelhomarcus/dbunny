/**
 * The slice of Markdown a release uses: headings, paragraphs, lists, fenced code,
 * tables, and inline code, bold and links. Enough to draw the changelog; not a
 * full Markdown implementation.
 */

export type Inline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "link"; text: string; href: string };

export type ReleaseBlock =
  | { kind: "heading"; level: number; inlines: Inline[] }
  | { kind: "paragraph"; inlines: Inline[] }
  | { kind: "list"; items: Inline[][] }
  | { kind: "code"; language: string; text: string }
  | { kind: "table"; header: Inline[][]; rows: Inline[][][] };

export function parseReleaseMarkdown(source: string): ReleaseBlock[] {
  const lines = source.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  const blocks: ReleaseBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    if (lines[i].trim() === "") {
      i++;
      continue;
    }

    if (lines[i].trimStart().startsWith("```")) {
      const language = lines[i].trimStart().slice(3).trim();
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        body.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++;
      blocks.push({ kind: "code", language, text: body.join("\n") });
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(lines[i]);
    if (heading) {
      blocks.push({ kind: "heading", level: heading[1].length, inlines: parseInlines(heading[2].trim()) });
      i++;
      continue;
    }

    const header = splitRow(lines[i]);
    const separator = i + 1 < lines.length ? splitRow(lines[i + 1]) : null;
    if (header && separator && isSeparator(separator)) {
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && lines[i].trim() !== "" && lines[i].includes("|")) {
        const row = splitRow(lines[i]);
        if (row) rows.push(row.map(parseInlines));
        i++;
      }
      blocks.push({ kind: "table", header: header.map(parseInlines), rows });
      continue;
    }

    if (/^\s*[-*]\s+/.test(lines[i])) {
      const items: Inline[][] = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) {
        const parts = [lines[i].replace(/^\s*[-*]\s+/, "")];
        i++;
        while (i < lines.length && /^\s+\S/.test(lines[i]) && !/^\s*[-*]\s+/.test(lines[i])) {
          parts.push(lines[i].trim());
          i++;
        }
        items.push(parseInlines(parts.join(" ")));
      }
      blocks.push({ kind: "list", items });
      continue;
    }

    const paragraph: string[] = [lines[i].trim()];
    i++;
    while (i < lines.length && lines[i].trim() !== "" && !startsBlock(lines, i)) {
      paragraph.push(lines[i].trim());
      i++;
    }
    blocks.push({ kind: "paragraph", inlines: parseInlines(paragraph.join(" ")) });
  }

  return blocks;
}

/** A line that begins a heading, list, code fence or table, so a paragraph stops before it. */
function startsBlock(lines: string[], index: number): boolean {
  const line = lines[index];
  if (line.trimStart().startsWith("```")) return true;
  if (/^(#{1,6})\s+/.test(line)) return true;
  if (/^\s*[-*]\s+/.test(line)) return true;
  const row = splitRow(line);
  const next = index + 1 < lines.length ? splitRow(lines[index + 1]) : null;
  return Boolean(row && next && isSeparator(next));
}

function splitRow(line: string): string[] | null {
  const trimmed = line.trim();
  if (!trimmed.includes("|")) return null;
  let body = trimmed;
  if (body.startsWith("|")) body = body.slice(1);
  if (body.endsWith("|")) body = body.slice(0, -1);
  return body.split("|").map((cell) => cell.trim());
}

function isSeparator(cells: string[]): boolean {
  return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
}

function parseInlines(input: string): Inline[] {
  const out: Inline[] = [];
  let text = "";
  let i = 0;
  const flush = () => {
    if (text) out.push({ kind: "text", text });
    text = "";
  };

  while (i < input.length) {
    if (input[i] === "`") {
      const end = input.indexOf("`", i + 1);
      if (end !== -1) {
        flush();
        out.push({ kind: "code", text: input.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }
    if (input.startsWith("**", i)) {
      const end = input.indexOf("**", i + 2);
      if (end !== -1) {
        flush();
        out.push({ kind: "strong", text: input.slice(i + 2, end) });
        i = end + 2;
        continue;
      }
    }
    if (input[i] === "[") {
      const match = /^\[([^\]]+)\]\(([^)\s]+)\)/.exec(input.slice(i));
      if (match) {
        flush();
        out.push({ kind: "link", text: match[1], href: match[2] });
        i += match[0].length;
        continue;
      }
    }
    text += input[i];
    i++;
  }
  flush();
  return out;
}
