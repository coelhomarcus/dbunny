import type { ConnectionDraft } from "../../lib/connectionDraft";
import Switch from "../ui/Switch";
import { useT, type StaticKey } from "../../i18n";

const READ_ONLY_HINT: Record<ConnectionDraft["engine"], StaticKey> = {
  postgres: "connect.optionsTab.readOnlyServerHint",
  sqlite: "connect.optionsTab.readOnlyFileHint",
  mysql: "connect.optionsTab.readOnlyServerHint",
  mariadb: "connect.optionsTab.readOnlyServerHint",
};

/** Settings every database has: for now, read-only. */
export default function OptionsTab({ draft, onChange }: { draft: ConnectionDraft; onChange: (draft: ConnectionDraft) => void }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between gap-4">
      <label htmlFor="conn-readonly" className="select-none">
        <span className="block text-[13px] font-medium">{t("connect.optionsTab.readOnly")}</span>
        <span className="block text-xs text-muted-foreground">{t(READ_ONLY_HINT[draft.engine])}</span>
      </label>
      <Switch
        id="conn-readonly"
        checked={draft.fields.readOnly}
        onChange={(v) => onChange({ ...draft, fields: { ...draft.fields, readOnly: v } })}
      />
    </div>
  );
}
