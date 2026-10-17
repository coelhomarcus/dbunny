import type { CompletionRelation, NewForeignKey } from "@/types";

/** Draft foreign key in the create-table or structure dialogs. */
export type ForeignKeyDraft = {
  refTable: string;
  columns: string[];
  refColumns: Record<string, string>;
  onUpdate: string;
  onDelete: string;
};

export const REFERENTIAL_ACTIONS = ["NO ACTION", "RESTRICT", "CASCADE", "SET NULL", "SET DEFAULT"] as const;

/** The referenced column a new foreign key column most likely points at. */
export function guessReference(column: string, target: CompletionRelation | undefined): string {
  const names = target?.columns.map((c) => c.name) ?? [];
  return names.find((n) => n === column) ?? names.find((n) => n === "id") ?? names[0] ?? "";
}

/** Why this card cannot be sent yet, or null when it is complete. */
export function foreignKeyDraftError(fk: ForeignKeyDraft, namedColumns: string[]): string | null {
  if (!fk.refTable.trim()) return "Choose a table";
  const locals = fk.columns.filter((c) => namedColumns.includes(c));
  if (locals.length === 0) return "Choose at least one column";
  return null;
}

/** Complete foreign keys for the create-table preview and command. Incomplete drafts are omitted. */
export function foreignKeysForSpec(drafts: ForeignKeyDraft[], relations: CompletionRelation[] | null, namedColumns: string[]): NewForeignKey[] {
  return drafts.flatMap((fk) => {
    if (foreignKeyDraftError(fk, namedColumns)) return [];
    const target = relations?.find((r) => `${r.schema}.${r.name}` === fk.refTable);
    if (!target) return [];
    const locals = fk.columns.filter((c) => namedColumns.includes(c));
    return [
      {
        columns: locals,
        refSchema: target.schema,
        refTable: target.name,
        refColumns: locals.map((c) => fk.refColumns[c] ?? guessReference(c, target)),
        onUpdate: fk.onUpdate,
        onDelete: fk.onDelete,
      },
    ];
  });
}
