import { useState } from "react";
import { getConnectionPrefs, setConnectionPrefs } from "../../lib/assistantPrefs";
import Button from "../ui/Button";
import Dialog from "../ui/Dialog";
import Switch from "../ui/Switch";

/** What the assistant is told about one saved connection: whether it may see values, and what to know about the data. */
export default function ConnectionOptionsDialog({ savedId, open, onClose }: { savedId: string; open: boolean; onClose: () => void }) {
  const [prefs, setPrefs] = useState(() => getConnectionPrefs(savedId));

  function save() {
    setConnectionPrefs(savedId, { ...prefs, instructions: prefs.instructions.trim() });
    onClose();
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Assistant options for this connection"
      description="Applies to new conversations."
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="text-[13px] font-medium">Schema only</div>
            <div className="text-xs text-muted-foreground">The assistant sees column names and row counts, never values.</div>
          </div>
          <Switch checked={prefs.schemaOnly} onChange={(schemaOnly) => setPrefs({ ...prefs, schemaOnly })} aria-label="Schema only" />
        </div>
        <label className="block">
          <span className="text-[13px] font-medium">Connection instructions</span>
          <textarea
            value={prefs.instructions}
            rows={4}
            placeholder="Facts about this database the assistant should know, e.g. status 3 means cancelled."
            onChange={(event) => setPrefs({ ...prefs, instructions: event.target.value })}
            className="mt-1 w-full resize-y rounded-lg border border-input bg-card px-2.5 py-2 text-[13px] outline-none placeholder:text-muted-foreground/70 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 dark:bg-muted"
          />
        </label>
      </div>
    </Dialog>
  );
}
