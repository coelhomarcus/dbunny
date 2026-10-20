import { useState } from "react";
import { getConnectionPrefs, setConnectionPrefs } from "../../lib/assistantPrefs";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import Switch from "../ui/Switch";
import { useT } from "../../i18n";

/** What the assistant is told about one saved connection: whether it may see values, and what to know about the data. */
export default function ConnectionOptionsDialog({ savedId, open, onClose }: { savedId: string; open: boolean; onClose: () => void }) {
  const t = useT();
  const [prefs, setPrefs] = useState(() => getConnectionPrefs(savedId));

  function save() {
    setConnectionPrefs(savedId, { ...prefs, instructions: prefs.instructions.trim() });
    onClose();
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("assistant.connectionOptionsDialog.assistantOptionsForThisConnection")}
      description={t("assistant.connectionOptionsDialog.appliesToNewConversations")}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" onClick={save}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="text-[13px] font-medium">{t("assistant.connectionOptionsDialog.schemaOnly")}</div>
            <div className="text-xs text-muted-foreground">{t("assistant.connectionOptionsDialog.theAssistantSeesColumnNames")}</div>
          </div>
          <Switch checked={prefs.schemaOnly} onChange={(schemaOnly) => setPrefs({ ...prefs, schemaOnly })} aria-label={t("assistant.connectionOptionsDialog.schemaOnly")} />
        </div>
        <label className="block">
          <span className="text-[13px] font-medium">{t("assistant.connectionOptionsDialog.connectionInstructions")}</span>
          <textarea
            value={prefs.instructions}
            rows={4}
            placeholder={t("assistant.connectionOptionsDialog.factsAboutThisDatabaseThe")}
            onChange={(event) => setPrefs({ ...prefs, instructions: event.target.value })}
            className="mt-1 w-full resize-y rounded-lg border border-input bg-card px-2.5 py-2 text-[13px] outline-none placeholder:text-muted-foreground/70 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 dark:bg-muted"
          />
        </label>
      </div>
    </Dialog>
  );
}
