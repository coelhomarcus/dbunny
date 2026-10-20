import { FilePlus, FolderOpen } from "lucide-react";
import { missingFields, type ConnectionDraft } from "../../../lib/connectionDraft";
import { fileNameOf } from "../../../lib/connectionSummary";
import Button from "../../ui/Button";
import Field from "../../ui/Field";
import Input from "../../ui/Input";
import { useT } from "../../../i18n";

/** The file name without its extension, to suggest as the connection's name. */
function suggestedName(path: string) {
  return fileNameOf(path).replace(/\.[^.]*$/, "");
}

/** The only tab SQLite needs: which database file to open, or where to make a new one. */
export default function SqliteFileTab({
  draft,
  onChange,
  showErrors,
}: {
  draft: ConnectionDraft;
  onChange: (draft: ConnectionDraft) => void;
  showErrors: boolean;
}) {
  const t = useT();
  const invalid = showErrors && missingFields(draft, "connect").some((m) => m.fieldId === "sqlite-path");

  /** Sets the file; a connection that still has no name takes the file's. */
  function pick(path: string, create: boolean) {
    const named = draft.identity.name.trim() ? draft.identity : { ...draft.identity, name: suggestedName(path) };
    onChange({ ...draft, sqlitePath: path, sqliteCreate: create, identity: named });
  }

  async function openFile() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({
      multiple: false,
      title: t("connect.sqliteFileTab.chooseTitle"),
      filters: [
        { name: t("connect.sqliteFileTab.sqliteDatabase"), extensions: ["db", "sqlite", "sqlite3", "db3"] },
        { name: t("connect.sqliteFileTab.allFiles"), extensions: ["*"] },
      ],
    });
    if (typeof path === "string") pick(path, false);
  }

  async function createFile() {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ title: t("connect.sqliteFileTab.createTitle"), defaultPath: "database.sqlite" });
    if (typeof path === "string") pick(path, true);
  }

  return (
    <div className="flex flex-col gap-4">
      <Field
        label={t("connect.sqliteFileTab.databaseFile")}
        htmlFor="sqlite-path"
        hint={draft.sqliteCreate ? t("connect.sqliteFileTab.createdHint") : t("connect.sqliteFileTab.chooseHint")}
      >
        <Input
          id="sqlite-path"
          autoFocus
          value={draft.sqlitePath}
          // Typing a path means opening it: a new database comes from "Create new…", so a typo
          // never makes an empty file
          onChange={(e) => onChange({ ...draft, sqlitePath: e.target.value, sqliteCreate: false })}
          placeholder={t("connect.sqliteFileTab.pathPlaceholder")}
          spellCheck={false}
          className="font-mono text-xs"
          aria-invalid={invalid || undefined}
        />
      </Field>
      <div className="flex gap-2">
        <Button variant="outline" onClick={openFile}>
          <FolderOpen />
          {t("connect.sqliteFileTab.open")}
        </Button>
        <Button variant="outline" onClick={createFile}>
          <FilePlus />
          {t("connect.sqliteFileTab.createNew")}
        </Button>
      </div>
    </div>
  );
}
