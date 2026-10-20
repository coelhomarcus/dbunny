import type { ConnectionDraft } from "../../lib/connectionDraft";
import Switch from "../ui/Switch";
import { useT } from "../../i18n";

const READ_ONLY_HINT: Record<ConnectionDraft["engine"], string> = {
  postgres: "The server refuses inserts, updates, deletes and schema changes",
  sqlite: "The database file is opened read-only: edits are refused",
  mysql: "The server refuses inserts, updates, deletes and schema changes",
  mariadb: "The server refuses inserts, updates, deletes and schema changes",
};

/** Settings every database has: for now, read-only. */
export default function OptionsTab({ draft, onChange }: { draft: ConnectionDraft; onChange: (draft: ConnectionDraft) => void }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between gap-4">
      <label htmlFor="conn-readonly" className="select-none">
        <span className="block text-[13px] font-medium">{t("connect.optionsTab.readOnly")}</span>
        <span className="block text-xs text-muted-foreground">{READ_ONLY_HINT[draft.engine]}</span>
      </label>
      <Switch
        id="conn-readonly"
        checked={draft.fields.readOnly}
        onChange={(v) => onChange({ ...draft, fields: { ...draft.fields, readOnly: v } })}
      />
    </div>
  );
}
