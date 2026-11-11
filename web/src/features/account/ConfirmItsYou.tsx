import { useRef, useState, type FormEvent } from "react";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import type { AccountConfirmAction } from "../../generated/AccountConfirmAction";
import type { AccountConnectPolicy } from "../../generated/AccountConnectPolicy";
import { accountApi } from "./api";
import { announceAccountChange } from "./changes";
import { Failure } from "./Card";
import { retryText } from "./codes";
import { NETWORK_TEXT, problemOf, type Problem } from "./problem";
import { useAfter, useCountdown, type Wait } from "./useCountdown";

/**
 * "Confirm it's you": the one dialog that asks for the password again before removing a device, signing out
 * everywhere and the other sensitive actions. A confirmation is for this one action (and device) and is used up
 * by it; the service refuses an action without one with `reauth_required`, whatever this dialog did.
 * `onConfirmed` runs the action.
 */
export function ConfirmItsYou({ action, deviceId, policy, subject, onConfirmed, onClose }: { action: AccountConfirmAction; deviceId?: string; policy?: AccountConnectPolicy; subject?: string; onConfirmed(): void | Promise<void>; onClose(): void }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [wait, setWait] = useState<Wait | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const spinning = useAfter(busy);
  const left = useCountdown(wait);
  const throttled = left !== null && left > 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || throttled || !password) return;
    setBusy(true);
    setProblem(null);
    try {
      await accountApi.confirm(password, action, deviceId, policy);
      setPassword("");
      await onConfirmed();
      onClose();
    } catch (error) {
      const found = problemOf(error);
      setProblem(found);
      // The action may have reached the Engine before it failed: read the account and its devices again.
      if (found.kind !== "wrong" && found.kind !== "throttled") announceAccountChange();
      // A wrong or refused password is cleared; the network failure keeps it typed.
      if (found.kind !== "network") setPassword("");
      if (found.kind === "throttled") setWait({ seconds: found.seconds, at: Date.now() });
      setBusy(false);
      requestAnimationFrame(() => field.current?.focus());
    }
  };

  const network = problem?.kind === "network";
  return (
    <Dialog
      title="Confirm it’s you"
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          <Button type="submit" form="confirm-its-you" tone="primary" busy={busy && spinning} disabled={busy || throttled || !password}>
            {busy && spinning ? "Checking…" : network ? "Try again" : "Confirm"}
          </Button>
        </>
      }
    >
      <form id="confirm-its-you" className="ac-form" onSubmit={(event) => void submit(event)}>
        <p>Enter your password to continue.</p>
        {subject ? <p className="ac-sub">{subject}</p> : null}
        {throttled ? <Failure>Too many attempts. Try again {retryText(left)}.</Failure> : null}
        {network ? <Failure>{NETWORK_TEXT}</Failure> : null}
        {problem?.kind === "other" ? <Failure>{problem.message}</Failure> : null}
        <Field label="Password" error={problem?.kind === "wrong" ? "That isn’t your password. Check it and try again." : null}>
          {(props) => (
            <input {...props} ref={field} type="password" autoComplete="current-password" data-dialog-focus value={password} disabled={busy}
              onChange={(event) => setPassword(event.target.value)} />
          )}
        </Field>
        <p className="ac-sub">{action === "remove_device" ? "This confirms removing this device." : "This confirms one action."}</p>
      </form>
    </Dialog>
  );
}
