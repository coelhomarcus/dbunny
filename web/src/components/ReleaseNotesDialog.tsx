import { useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import Spinner from "./ui/Spinner";
import { fetchReleaseNotes } from "../lib/releaseNotes";
import Dialog from "./ui/Dialog";
import Button from "./ui/Button";
import ReleaseNotes from "./ReleaseNotes";
import { useT } from "../i18n";

/**
 * The changelog of one release, loaded when the dialog opens and drawn from its Markdown.
 */
export default function ReleaseNotesDialog({
  open,
  version,
  onClose,
}: {
  open: boolean;
  version: string;
  onClose: () => void;
}) {
  const t = useT();
  const [notes, setNotes] = useState<{ version: string; markdown: string } | { version: string; error: string } | null>(null);

  useEffect(() => {
    if (!open) return;
    // Before the dialog and anything under it, so Escape closes the notes and leaves Settings open.
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onClose();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    fetchReleaseNotes(version).then(
      (markdown) => {
        if (!cancelled) setNotes({ version, markdown });
      },
      (err) => {
        if (!cancelled) setNotes({ version, error: err instanceof Error ? err.message : t("ui.releaseNotesDialog.loadFailed") });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [open, version, t]);

  const current = notes?.version === version ? notes : null;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("ui.releaseNotesDialog.whatsNew")}
      description={`DBunny ${version.replace(/^v/, "")}`}
      className="max-w-2xl"
      overlayClassName="z-[70]"
      footer={
        <Button variant="outline" size="sm" onClick={onClose}>
          {t("common.close")}
        </Button>
      }
    >
      <div className="min-h-24">
        {!current && (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner className="size-4" /> {t("ui.releaseNotesDialog.loading")}
          </p>
        )}
        {current && "error" in current && (
          <p className="flex items-start gap-2 text-sm text-muted-foreground">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive-foreground" />
            <span className="select-text">{current.error}</span>
          </p>
        )}
        {current && "markdown" in current && <ReleaseNotes markdown={current.markdown} />}
      </div>
    </Dialog>
  );
}
