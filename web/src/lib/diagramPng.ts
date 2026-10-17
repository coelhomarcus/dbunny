import type { DiagramColumn } from "@/types";
import { TABLE_WIDTH, columnCenterY, relationLabelAt, relationRoute, tableHeight } from "./diagramLayout";

export interface PngTable {
  id: string;
  x: number;
  y: number;
  name: string;
  schema: string;
  showSchema: boolean;
  columns: DiagramColumn[];
}

export interface PngRelation {
  source: string;
  target: string;
  sourceColumn: string;
  targetColumn: string;
  label: string;
}

const PADDING = 48;
const CANVAS_LIMIT = 16384;

function themeOf(dark: boolean) {
  return dark
    ? {
        background: "#161617",
        card: "#111112",
        border: "#2e2e32",
        text: "#f4f4f5",
        muted: "#8b8b94",
        key: "#fbbf24",
        foreign: "#60a5fa",
        line: "#71717a",
      }
    : {
        background: "#f4f4f5",
        card: "#ffffff",
        border: "#e4e4e7",
        text: "#18181b",
        muted: "#71717a",
        key: "#b45309",
        foreign: "#1d4ed8",
        line: "#71717a",
      };
}

function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let trimmed = text;
  while (trimmed.length > 1 && ctx.measureText(`${trimmed}…`).width > max) trimmed = trimmed.slice(0, -1);
  return `${trimmed}…`;
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number) {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + width, y, x + width, y + height, radius);
  ctx.arcTo(x + width, y + height, x, y + height, radius);
  ctx.arcTo(x, y + height, x, y, radius);
  ctx.arcTo(x, y, x + width, y, radius);
  ctx.closePath();
}

function strokeRoute(ctx: CanvasRenderingContext2D, points: { x: number; y: number }[]) {
  const radius = 8;
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1];
    const curr = points[i];
    const next = points[i + 1];
    const inX = curr.x - prev.x;
    const inY = curr.y - prev.y;
    const outX = next.x - curr.x;
    const outY = next.y - curr.y;
    const inLen = Math.hypot(inX, inY) || 1;
    const outLen = Math.hypot(outX, outY) || 1;
    const r = Math.min(radius, inLen / 2, outLen / 2);
    ctx.lineTo(curr.x - (inX / inLen) * r, curr.y - (inY / inLen) * r);
    ctx.quadraticCurveTo(curr.x, curr.y, curr.x + (outX / outLen) * r, curr.y + (outY / outLen) * r);
  }
  const last = points[points.length - 1];
  ctx.lineTo(last.x, last.y);
  ctx.stroke();
}

function arrow(ctx: CanvasRenderingContext2D, x: number, y: number) {
  ctx.beginPath();
  ctx.moveTo(x, y);
  ctx.lineTo(x - 8, y - 4);
  ctx.lineTo(x - 8, y + 4);
  ctx.closePath();
  ctx.fill();
}

/** The whole diagram, including tables scrolled out of view, as PNG bytes. */
export async function renderDiagramPng(tables: PngTable[], relations: PngRelation[], dark: boolean): Promise<Uint8Array> {
  if (tables.length === 0) throw new Error("Couldn't export the diagram");
  const theme = themeOf(dark);
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const table of tables) {
    minX = Math.min(minX, table.x);
    minY = Math.min(minY, table.y);
    maxX = Math.max(maxX, table.x + TABLE_WIDTH);
    maxY = Math.max(maxY, table.y + tableHeight(table.columns.length, table.showSchema));
  }
  const width = maxX - minX + PADDING * 2;
  const height = maxY - minY + PADDING * 2;
  let scale = 2;
  if (width * scale > CANVAS_LIMIT) scale = CANVAS_LIMIT / width;
  if (height * scale > CANVAS_LIMIT) scale = Math.min(scale, CANVAS_LIMIT / height);

  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.ceil(width * scale));
  canvas.height = Math.max(1, Math.ceil(height * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Couldn't export the diagram");
  ctx.scale(scale, scale);
  ctx.translate(PADDING - minX, PADDING - minY);
  ctx.fillStyle = theme.background;
  ctx.fillRect(minX - PADDING, minY - PADDING, width, height);
  ctx.textBaseline = "middle";

  const byId = new Map(tables.map((table) => [table.id, table]));
  const font = "ui-sans-serif, system-ui, sans-serif";

  for (const table of tables) {
    const height = tableHeight(table.columns.length, table.showSchema);
    ctx.fillStyle = theme.card;
    ctx.strokeStyle = theme.border;
    ctx.lineWidth = 1;
    roundRect(ctx, table.x, table.y, TABLE_WIDTH, height, 8);
    ctx.fill();
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(table.x, table.y + 32);
    ctx.lineTo(table.x + TABLE_WIDTH, table.y + 32);
    ctx.stroke();

    ctx.fillStyle = theme.text;
    ctx.font = `600 13px ${font}`;
    ctx.textAlign = "left";
    ctx.fillText(fit(ctx, table.name, TABLE_WIDTH - 20), table.x + 10, table.y + 16);

    if (table.showSchema) {
      ctx.fillStyle = theme.muted;
      ctx.font = `10px ${font}`;
      ctx.fillText(fit(ctx, table.schema, TABLE_WIDTH - 20), table.x + 10, table.y + 32 + 8);
    }

    table.columns.forEach((column, index) => {
      const y = table.y + columnCenterY(index, table.showSchema);
      const mark = column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "";
      if (mark) {
        ctx.fillStyle = column.isPrimaryKey ? theme.key : theme.foreign;
        ctx.font = `600 10px ${font}`;
        ctx.textAlign = "left";
        ctx.fillText(mark, table.x + 8, y);
      }
      ctx.font = `10px ${font}`;
      ctx.fillStyle = theme.muted;
      ctx.textAlign = "right";
      const type = fit(ctx, column.typeName, 96);
      const typeWidth = ctx.measureText(type).width;
      ctx.fillText(type, table.x + TABLE_WIDTH - 8, y);

      ctx.font = `12px ${font}`;
      ctx.fillStyle = theme.text;
      ctx.textAlign = "left";
      const nameMax = TABLE_WIDTH - 8 - 24 - 6 - 6 - typeWidth - 8;
      ctx.fillText(fit(ctx, column.name, Math.max(24, nameMax)), table.x + 8 + 24 + 6, y);

      ctx.fillStyle = theme.line;
      ctx.beginPath();
      ctx.arc(table.x, y, 3, 0, Math.PI * 2);
      ctx.arc(table.x + TABLE_WIDTH, y, 3, 0, Math.PI * 2);
      ctx.fill();
    });
  }

  ctx.strokeStyle = theme.line;
  ctx.fillStyle = theme.line;
  ctx.lineWidth = 1.25;
  ctx.lineJoin = "round";
  for (const relation of relations) {
    const source = byId.get(relation.source);
    const target = byId.get(relation.target);
    if (!source || !target) continue;
    const sourceIndex = source.columns.findIndex((column) => column.name === relation.sourceColumn);
    const targetIndex = target.columns.findIndex((column) => column.name === relation.targetColumn);
    if (sourceIndex < 0 || targetIndex < 0) continue;
    const sx = source.x + TABLE_WIDTH;
    const sy = source.y + columnCenterY(sourceIndex, source.showSchema);
    const tx = target.x;
    const ty = target.y + columnCenterY(targetIndex, target.showSchema);
    const points = relationRoute(sx, sy, tx, ty);
    points[points.length - 1] = { x: tx - 7, y: ty };
    strokeRoute(ctx, points);
    arrow(ctx, tx, ty);
    if (relation.label) {
      const at = relationLabelAt(relationRoute(sx, sy, tx, ty));
      ctx.font = `10px ${font}`;
      const text = fit(ctx, relation.label, 180);
      const textWidth = ctx.measureText(text).width;
      ctx.fillStyle = theme.card;
      roundRect(ctx, at.x - textWidth / 2 - 4, at.y - 8, textWidth + 8, 16, 4);
      ctx.fill();
      ctx.fillStyle = theme.muted;
      ctx.textAlign = "center";
      ctx.fillText(text, at.x, at.y);
    }
  }

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("Couldn't export the diagram");
  return new Uint8Array(await blob.arrayBuffer());
}
