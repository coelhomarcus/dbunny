import { useState } from "react";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";
import type { Engine } from "@/types";
import { urlHints } from "../../lib/connectionDraft";
import { useT } from "../../i18n";

/**
 * Asks for a connection URL. `onImport` returns false when it isn't one, to say so. `engine` is the
 * database the form is for; without it any server database's URL is welcome.
 */
export default function ImportUrlDialog({
  open,
  onClose,
  onImport,
  engine,
}: {
  open: boolean;
  onClose: () => void;
  onImport: (url: string) => boolean;
  engine?: Engine;
}) {
  const t = useT();
  const hints = urlHints(engine);
  const [url, setUrl] = useState("");
  const [invalid, setInvalid] = useState(false);

  function close() {
    setUrl("");
    setInvalid(false);
    onClose();
  }

  function submit() {
    if (!onImport(url)) {
      setInvalid(true);
      return;
    }
    setUrl("");
    setInvalid(false);
  }

  return (
    <Dialog
      open={open}
      onClose={close}
      title={t("connect.importUrlDialog.importFromUrl")}
      description={hints.description}
      footer={
        <>
          <Button variant="neutral" size="sm" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" size="sm" onClick={submit} disabled={!url.trim()}>
            {t("connect.importUrlDialog.import")}
          </Button>
        </>
      }
    >
      <Input
        autoFocus
        value={url}
        onChange={(e) => {
          setUrl(e.target.value);
          setInvalid(false);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            submit();
          }
        }}
        placeholder={hints.placeholder}
        spellCheck={false}
        className="font-mono text-xs"
        aria-invalid={invalid}
      />
      {invalid && <p className="mt-2 select-text text-xs text-destructive-foreground">{hints.invalid}</p>}
    </Dialog>
  );
}
