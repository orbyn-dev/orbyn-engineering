import { useRef, useState, type ReactNode } from "react";
import { toFailure } from "../../lib/useAsync";
import { Button, type ButtonTone } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";

/**
 * A dialog for one write. One click on the confirm button is one write: the ref refuses a second click before the first
 * renders, `busy` blocks Escape, Cancel and the button while it runs, and a failure keeps the dialog open and says why.
 * Nothing runs on open, and nothing runs unless the person presses the confirm button.
 */
export function ActionDialog({ title, confirmLabel, cancelLabel = "Cancel", tone = "primary", disabled, cancelFocus, onConfirm, onClose, children }: {
  title: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  tone?: ButtonTone;
  disabled?: boolean;
  cancelFocus?: boolean;
  onConfirm(): Promise<void>;
  onClose(): void;
  children: ReactNode;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);
  const confirm = async () => {
    if (running.current || disabled) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      running.current = false;
      setBusy(false);
    }
  };
  return (
    <Dialog
      title={title}
      busy={busy}
      onClose={onClose}
      className="repos-dialog"
      footer={<>
        <Button onClick={onClose} disabled={busy} data-dialog-focus={cancelFocus || undefined}>{cancelLabel}</Button>
        <Button tone={tone} busy={busy} disabled={disabled} onClick={() => void confirm()}>{confirmLabel}</Button>
      </>}
    >
      {children}
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </Dialog>
  );
}
