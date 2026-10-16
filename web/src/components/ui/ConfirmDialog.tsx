import type { ReactNode } from "react";
import Button, { type ButtonProps } from "./Button";
import Dialog from "./Dialog";

type Tone = "info" | "warning" | "destructive";

// The confirm button takes the dialog's colour
const CONFIRM_VARIANT: Record<Tone, ButtonProps["variant"]> = {
  info: "primary",
  warning: "warning",
  destructive: "destructive",
};

/** "Are you sure?" modal: a coloured icon, a neutral way out and a confirm button in the tone's colour. */
export default function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  icon,
  tone = "warning",
  title,
  description,
  children,
  cancelLabel = "Cancel",
  confirmLabel = "Confirm",
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  icon: ReactNode;
  tone?: Tone;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  cancelLabel?: string;
  confirmLabel?: string;
}) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      icon={icon}
      tone={tone}
      title={title}
      description={description}
      footer={
        <>
          {/* Enter must never confirm something destructive by accident */}
          <Button variant="neutral" size="sm" onClick={onClose} autoFocus={tone === "destructive"}>
            {cancelLabel}
          </Button>
          <Button variant={CONFIRM_VARIANT[tone]} size="sm" onClick={onConfirm} autoFocus={tone !== "destructive"}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}
