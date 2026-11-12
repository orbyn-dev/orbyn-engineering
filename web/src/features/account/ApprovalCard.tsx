import { Loader2 } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import type { AccountApprovalView } from "../../generated/AccountApprovalView";
import { Button } from "../../ui/controls";
import { accountApi } from "./api";
import { Card } from "./Card";
import { DigitBoxes } from "./DigitBoxes";
import { useAfter } from "./useCountdown";

const POLL_MS = 2000;
/** Failed looks in a row before the screen stops saying it is waiting. */
const LOOK_FAILURES = 3;
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
const triesText = (left: number) => `${left} ${left === 1 ? "try" : "tries"} left.`;

/**
 * This computer's own request to sign in, while a trusted device decides: a wait that says how long is left, then,
 * once a trusted device allows it, six boxes for the six digits that device shows. The digits are never shown here and
 * nothing is filled in: only the right ones sign in, and an allowed request never finishes by itself. The service
 * answers each look at the request, and counts the tries.
 */
export function ApprovalCard({ approval, onChanged, onBack }: { approval: AccountApprovalView; onChanged(): void; onBack(to: "sign-in" | "home"): void }) {
  const [shown, setShown] = useState(approval);
  // The time left is the service's, taken again on every look and counted down here between looks by ticks, not by
  // the wall clock: a laptop that slept does not lose or regain minutes.
  const [left, setLeft] = useState(approval.expires_in_secs);
  const [failures, setFailures] = useState(0);
  const [busy, setBusy] = useState(false);
  const [typed, setTyped] = useState("");
  // Bumped when wrong digits cleared the boxes, so the first box is ready for typing again.
  const [attempt, setAttempt] = useState(0);
  const open = shown.state === "waiting" || shown.state === "enter_digits";
  const spinning = useAfter(shown.state === "waiting" || shown.state === "approved");
  // The waiting card takes the focus once; when looks fail and then succeed it must not take it back.
  const focused = useRef(false);
  useEffect(() => {
    if (shown.state === "waiting") focused.current = true;
  }, [shown.state]);
  const changed = useRef(onChanged);
  changed.current = onChanged;

  useEffect(() => {
    if (!open) return;
    const tick = setInterval(() => setLeft((was) => Math.max(0, was - 1)), 1000);
    const look = setInterval(() => {
      void accountApi
        .pollApproval()
        .then((next) => {
          setFailures(0);
          if (!next) return changed.current();
          setShown(next);
          setLeft(next.expires_in_secs);
        })
        .catch(() => setFailures((count) => count + 1));
    }, POLL_MS);
    return () => {
      clearInterval(tick);
      clearInterval(look);
    };
  }, [open]);

  const leave = async (to: "sign-in" | "home") => {
    setBusy(true);
    await accountApi.cancelSignIn().catch(() => undefined);
    onBack(to);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || typed.length !== 6) return;
    setBusy(true);
    try {
      const next = await accountApi.submitDigits(typed);
      setFailures(0);
      if (!next) return changed.current();
      setShown(next);
      setLeft(next.expires_in_secs);
      // Wrong digits clear the boxes: the typed entry is never kept, and a new one starts in the first box.
      setTyped("");
      if (next.state === "enter_digits") setAttempt((count) => count + 1);
      if (next.state === "approved") changed.current();
    } catch {
      // The service could not be reached, so the entry was not used: the screen says it cannot check, and looks go on.
      setTyped("");
      setFailures(LOOK_FAILURES);
    } finally {
      setBusy(false);
    }
  };

  if (shown.state === "approved") {
    return (
      <Card focus title="Approved" lead="A device you trust allowed this sign-in. Finishing up…" label="Sign in · approve this device">
        <p className="ac-wait" role="status">
          <Loader2 size={12} className="spin" aria-hidden />
          Signing you in…
        </p>
      </Card>
    );
  }
  if (shown.state === "timed_out") {
    return (
      <Card focus alert title="This request expired" lead="A request lasts 10 minutes. Open Orbyn on one of your trusted devices and try again. The digits change each time." label="Sign in · approve this device">
        <div className="ac-acts">
          <Button tone="primary" disabled={busy} onClick={() => void leave("sign-in")}>Try again</Button>
          <Button disabled={busy} onClick={() => void leave("home")}>Back to Account</Button>
        </div>
      </Card>
    );
  }
  if (shown.state === "locked") {
    return (
      <Card focus alert title="Too many tries. This request is closed." lead="No one is signed in on this device. If this was you, try again, or sign in from a device you already trust." label="Sign in · approve this device">
        <div className="ac-acts">
          <Button tone="primary" disabled={busy} onClick={() => void leave("sign-in")}>Try again</Button>
          <Button disabled={busy} onClick={() => void leave("home")}>Back to Account</Button>
        </div>
      </Card>
    );
  }
  if (shown.state === "denied") {
    // The Engine says nothing about why a request closed on the other side, so every such close reads the same.
    return <ClosedCard busy={busy} onLeave={(to) => void leave(to)} />;
  }
  if (failures >= LOOK_FAILURES) {
    // Looking has failed several times in a row. Nothing is closed: the request may still be waiting, and the looks go
    // on, so this reads neutral (no alert, no spinner) and goes back to waiting when one succeeds.
    return (
      <Card title="Couldn’t check this request." lead="This isn’t final: the request may still be waiting on your other device." label="Sign in · approve this device">
        <div className="ac-acts">
          <Button disabled={busy} onClick={() => void leave("sign-in")}>Cancel</Button>
        </div>
      </Card>
    );
  }
  if (shown.state === "enter_digits") {
    const wrong = shown.tries_left !== undefined;
    return (
      <Card focus={false} title="Type the digits from your other device" lead="Type the six digits shown on your other device." label="Sign in · approve this device">
        <form className="ac-form" onSubmit={(event) => void submit(event)}>
          <DigitBoxes value={typed} onChange={setTyped} disabled={busy} bad={wrong} resetKey={attempt} />
          {wrong ? (
            <p className="ac-tries" role="alert">
              <span className="ac-bad">Those digits don’t match.</span>
              <span className="ac-left">{triesText(shown.tries_left ?? 0)}</span>
            </p>
          ) : null}
          <p className="ac-wait ac-center">This request expires in {clock(left)}</p>
          <div className="ac-acts">
            <Button type="submit" tone="primary" busy={busy} disabled={busy || typed.length !== 6}>Continue</Button>
            <Button disabled={busy} onClick={() => void leave("sign-in")}>Cancel</Button>
          </div>
        </form>
      </Card>
    );
  }
  return (
    <Card focus={!focused.current} title="Approve this sign-in on another device" lead="We asked your other devices to approve this one. Open Orbyn on a device you trust and allow it. That device then shows six digits to type here." label="Sign in · approve this device">
      <p className="ac-wait">
        {spinning ? <Loader2 size={12} className="spin" aria-hidden /> : null}
        <span role="status">Waiting for approval</span> · this request expires in {clock(left)}
      </p>
      <div className="ac-acts">
        <Button disabled={busy} onClick={() => void leave("sign-in")}>Cancel</Button>
      </div>
    </Card>
  );
}

/** Every closed request, whichever side closed it: one neutral message, because the screen does not know which. */
function ClosedCard({ busy = false, onLeave }: { busy?: boolean; onLeave(to: "sign-in" | "home"): void }) {
  return (
    <Card focus alert title="Denied or cancelled." lead="A device you trust said no, or the request was cancelled. If this was you, try again, or sign in from a device you already trust." label="Sign in · approve this device">
      <div className="ac-acts">
        <Button tone="primary" disabled={busy} onClick={() => onLeave("sign-in")}>Try again</Button>
        <Button disabled={busy} onClick={() => onLeave("home")}>Back to Account</Button>
      </div>
    </Card>
  );
}
