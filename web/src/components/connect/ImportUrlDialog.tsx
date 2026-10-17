import { useState } from "react";
import Dialog from "../ui/Dialog";
import Button from "../ui/Button";
import Input from "../ui/Input";

/** Asks for a connection URL. `onImport` returns false when it isn't one, to say so. */
export default function ImportUrlDialog({
  open,
  onClose,
  onImport,
}: {
  open: boolean;
  onClose: () => void;
  onImport: (url: string) => boolean;
}) {
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
      title="Import from URL"
      description="Paste a PostgreSQL connection string to fill in the form."
      footer={
        <>
          <Button variant="neutral" size="sm" onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={submit} disabled={!url.trim()}>
            Import
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
        placeholder="postgresql://user:password@host:5432/database"
        spellCheck={false}
        className="font-mono text-xs"
        aria-invalid={invalid}
      />
      {invalid && <p className="mt-2 select-text text-xs text-destructive-foreground">That doesn't look like a postgres:// connection URL.</p>}
    </Dialog>
  );
}
