import { Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AccountApprovalView } from "../../generated/AccountApprovalView";
import { Button } from "../../ui/controls";
import { accountApi } from "./api";
import { Card } from "./Card";
import { useAfter } from "./useCountdown";

const POLL_MS = 2000;
/** Failed looks in a row before the screen stops saying it is waiting. */
const LOOK_FAILURES = 3;
/** "418 207" read as digits in two groups: "4 1 8, 2 0 7". */
const spoken = (digits: string) => `Code ${digits.split(" ").map((group) => [...group].join(" ")).join(", ")}`;
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

/**
 * This computer's own request to sign in, while a trusted device decides: the digits to compare, a wait that says how
 * long is left, and what ended it. The service answers each look at the request, and an allowed one signs in.
 */
export function ApprovalCard({ approval, onChanged, onBack, onMismatch }: { approval: AccountApprovalView; onChanged(): void; onBack(to: "sign-in" | "home"): void; onMismatch(): void }) {
  const [shown, setShown] = useState(approval);
  // The time left is the service's, taken again on every look and counted down here between looks by ticks, not by
  // the wall clock: a laptop that slept does not lose or regain minutes.
  const [left, setLeft] = useState(approval.expires_in_secs);
  const [failures, setFailures] = useState(0);
  const [busy, setBusy] = useState(false);
  const spinning = useAfter(shown.state === "waiting" || shown.state === "approved");
  // The waiting card takes the focus once; when looks fail and then succeed it must not take it back.
  const focused = useRef(false);
  useEffect(() => {
    if (shown.state === "waiting") focused.current = true;
  }, [shown.state]);
  const changed = useRef(onChanged);
  changed.current = onChanged;

  useEffect(() => {
    if (shown.state !== "waiting") return;
    const tick = setInterval(() => setLeft((was) => Math.max(0, was - 1)), 1000);
    const look = setInterval(() => {
      void accountApi
        .pollApproval()
        .then((next) => {
          setFailures(0);
          if (!next) return changed.current();
          setShown(next);
          setLeft(next.expires_in_secs);
          if (next.state === "approved") changed.current();
        })
        .catch(() => setFailures((count) => count + 1));
    }, POLL_MS);
    return () => {
      clearInterval(tick);
      clearInterval(look);
    };
  }, [shown.state]);

  const leave = async (to: "sign-in" | "home") => {
    setBusy(true);
    await accountApi.cancelSignIn().catch(() => undefined);
    onBack(to);
  };
  const mismatch = async () => {
    setBusy(true);
    await accountApi.cancelSignIn().catch(() => undefined);
    onMismatch();
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
      <Card focus alert title="No one approved this sign-in in time" lead="A request expires after 10 minutes. Open Orbyn on one of your trusted devices and try again. The code on this screen changes each time." label="Sign in · approve this device">
        <div className="ac-acts">
          <Button tone="primary" disabled={busy} onClick={() => void leave("sign-in")}>Try again</Button>
          <Button disabled={busy} onClick={() => void leave("home")}>Back to Account</Button>
        </div>
      </Card>
    );
  }
  if (shown.state === "denied" || shown.state === "codes_differ") {
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
  return (
    <Card focus={!focused.current} title="Approve this sign-in on another device" lead="We asked your other devices to approve this one. Open Orbyn on a device you trust, compare the codes and allow it." label="Sign in · approve this device">
      <div className="ac-digits">
        <small>Check that this code matches</small>
        <b aria-label={spoken(shown.digits)}>{shown.digits}</b>
        <small>Your other device shows the same code. Both screens must match.</small>
      </div>
      <p className="ac-wait">
        {spinning ? <Loader2 size={12} className="spin" aria-hidden /> : null}
        <span role="status">Waiting for approval</span> · this request expires in {clock(left)}
      </p>
      <div className="ac-acts">
        <Button disabled={busy} onClick={() => void leave("sign-in")}>Cancel</Button>
        <Button disabled={busy} onClick={() => void mismatch()}>They don’t match</Button>
      </div>
    </Card>
  );
}

/** Every closed request, whichever side closed it: one neutral message, because the screen does not know which. */
export function ClosedCard({ busy = false, onLeave }: { busy?: boolean; onLeave(to: "sign-in" | "home"): void }) {
  return (
    <Card focus alert title="Denied or cancelled." lead="A device you trust said no, or the request was cancelled. If this was you, try again, or sign in from a device you already trust." label="Sign in · approve this device">
      <div className="ac-acts">
        <Button tone="primary" disabled={busy} onClick={() => onLeave("sign-in")}>Try again</Button>
        <Button disabled={busy} onClick={() => onLeave("home")}>Back to Account</Button>
      </div>
    </Card>
  );
}
