import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
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
 * `onConfirmed` runs the action with the nonce the service answered with, which it must send back: only this window
 * has it. Cancel and Escape give a confirmation up, so an abandoned prompt does not leave one live.
 */
interface ConfirmItsYouProps {
  action: AccountConfirmAction;
  deviceId?: string;
  policy?: AccountConnectPolicy;
  /** What the confirmation is for: a line, or the facts (who asks, the command) a person reads before typing. */
  subject?: ReactNode;
  /** The confirming button, "Confirm" unless the action has its own name. */
  confirmLabel?: string;
  /** Said after the network failure's own sentence, for example that a request is still held. */
  networkNote?: string;
  /** Enter on an empty password closes the dialog, as Cancel does. */
  cancelOnEmptyEnter?: boolean;
  /** Runs the action with the nonce. Resolving `false` says the action did not use it, so it is given up. */
  onConfirmed(nonce: string): void | boolean | Promise<void | boolean>;
  onClose(): void;
}

export function ConfirmItsYou({ action, deviceId, policy, subject, confirmLabel = "Confirm", networkNote, cancelOnEmptyEnter, onConfirmed, onClose }: ConfirmItsYouProps) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [wait, setWait] = useState<Wait | null>(null);
  const field = useRef<HTMLInputElement>(null);
  // The confirmation this dialog made and has not used: dropped if the dialog goes away without using it.
  const held = useRef<string | null>(null);
  const spinning = useAfter(busy);
  const left = useCountdown(wait);
  const throttled = left !== null && left > 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || throttled || !password) return;
    setBusy(true);
    setProblem(null);
    // A retry makes a new confirmation: the one an earlier try made and did not use is given up first.
    const earlier = held.current;
    held.current = null;
    if (earlier) void accountApi.cancelConfirmation(earlier).catch(() => undefined);
    try {
      const { nonce } = await accountApi.confirm(password, action, deviceId, policy);
      held.current = nonce;
      setPassword("");
      const used = await onConfirmed(nonce);
      if (used === false) {
        held.current = null;
        void accountApi.cancelConfirmation(nonce).catch(() => undefined);
      }
      held.current = null;
      onClose();
    } catch (error) {
      const found = problemOf(error);
      setProblem(found);
      // The action may have reached the Engine before it failed: read the account and its devices again.
      if (found.kind !== "wrong" && found.kind !== "throttled") announceAccountChange();
      // A wrong or refused password is cleared; the network failure keeps it typed so Try again repeats it.
      if (found.kind !== "network") setPassword("");
      if (found.kind === "throttled") setWait({ seconds: found.seconds, at: Date.now() });
      setBusy(false);
      requestAnimationFrame(() => field.current?.focus());
    }
  };

  // The page took the dialog away (the session ended, a navigation): the confirmation it made and did not use is dropped too.
  useEffect(
    () => () => {
      const nonce = held.current;
      held.current = null;
      if (nonce) void accountApi.cancelConfirmation(nonce).catch(() => undefined);
    },
    [],
  );
  const giveUp = () => {
    const nonce = held.current;
    held.current = null;
    if (nonce) void accountApi.cancelConfirmation(nonce).catch(() => undefined);
    onClose();
  };
  const network = problem?.kind === "network";
  const emptyEnter = (event: KeyboardEvent) => {
    if (cancelOnEmptyEnter && event.key === "Enter" && !password && !busy) {
      event.preventDefault();
      giveUp();
    }
  };
  return (
    <Dialog
      title="Confirm it’s you"
      onClose={giveUp}
      busy={busy}
      footer={
        <>
          <Button onClick={giveUp} disabled={busy}>Cancel</Button>
          <Button type="submit" form="confirm-its-you" tone="primary" busy={busy && spinning} disabled={busy || throttled || !password}>
            {busy && spinning ? "Checking…" : network ? "Try again" : confirmLabel}
          </Button>
        </>
      }
    >
      <form id="confirm-its-you" className="ac-form" onSubmit={(event) => void submit(event)} onKeyDown={emptyEnter}>
        <p>Enter your password to continue.</p>
        {typeof subject === "string" ? <p className="ac-sub">{subject}</p> : subject ? <div className="ac-subject">{subject}</div> : null}
        {throttled ? <Failure>Too many attempts. Try again {retryText(left)}.</Failure> : null}
        {network ? <Failure>{networkNote ? `${NETWORK_TEXT} ${networkNote}` : NETWORK_TEXT}</Failure> : null}
        {problem?.kind === "gone" ? <Failure>Denied or cancelled.</Failure> : null}
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
