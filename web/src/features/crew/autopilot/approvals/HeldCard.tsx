// The held-command card: what the drawer shows for a command the guard holds for you. Approve once opens the
// desktop app's own native dialog (not drawn here); in a browser tab it is not offered. The words and states are
// the Crew designs' (v34): approval-confirm, -browser, -refused-presence, -expired and -decided.
import { Info } from "lucide-react";
import { Button } from "../../../../ui/controls";
import { GUARD_WORKERS, HELD, MODE_COPY } from "./copy";
import { decidedLine, phaseOf, type HoldControl } from "./held";
import type { ApprovalRow, HeldInfo, Mode } from "./model";
import { Facts } from "./parts";

const UNREADABLE = "This command is too long to read whole, so it cannot be approved once.";

export function Said({ children, note }: { children: string; note?: string | null }) {
  return (
    <section className="ap-box" data-quiet>
      <div className="ap-said"><Info size={14} aria-hidden /><span>{children}</span></div>
      {note ? <p>{note}</p> : null}
    </section>
  );
}

/** The service's reason as a sentence, then the designs' line that nothing runs until the person approves. */
const why = (reason: string) => {
  const text = reason.trim();
  return text ? `${/[.!?]$/.test(text) ? text : `${text}.`} ${HELD.why}` : HELD.why;
};

/** Why Approve once is off, when it is: the one reason a person can act on. */
function approveReason(held: HeldInfo, hold: HoldControl): string | null {
  if (hold.remote) return HELD.remote;
  if (!hold.desktop) return null;
  if (!held.readable) return UNREADABLE;
  return null;
}

export function HeldBody({ row, held, hold, mode }: { row: ApprovalRow; held: HeldInfo; hold: HoldControl; mode: Mode }) {
  const phase = phaseOf(held, hold.closed ?? undefined);
  const decided = decidedLine(held.state, held.decidedAt, held.byYou);
  // A browser tab cannot approve once; a remote window is answered at the computer the app runs on.
  const browserTab = !hold.desktop && !hold.remote;
  return (
    <>
      <p className="ap-asked">Task <b>{row.taskId}</b></p>
      <Facts rows={[
        ...(phase === "open" ? [{ label: "Autopilot", text: `${MODE_COPY[mode].name}: the guard holds this command for you` }] : []),
        { label: "Waiting since", text: held.since },
        ...(phase === "decided" && decided.fact ? [{ label: "Decided", text: decided.fact }] : []),
      ]} />
      {row.block ? <pre className="ap-cmd"><code>{row.block}</code></pre> : null}
      {phase === "expired" ? <Said>{HELD.expired}</Said> : null}
      {phase === "decided" ? <Said note={decided.sentence ? `${decided.sentence}${held.state === "denied" || held.state === "dismissed" ? " Nothing ran." : ""}` : null}>{HELD.decided}</Said> : null}
      {phase === "open" && browserTab ? <Said note={HELD.desktopOnlyNote}>{HELD.desktopOnly}</Said> : null}
      {phase === "open" && hold.refused ? <Said note={HELD.refusedNote}>{HELD.refused}</Said> : null}
      {phase === "open" && !browserTab && !hold.refused ? (
        <section className="ap-box">
          <h4>Why it is held</h4>
          <p>{why(held.reason)}</p>
        </section>
      ) : null}
      <p className="ap-muted">{HELD.restart}</p>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </>
  );
}

export function HeldFooter({ held, hold, close }: { held: HeldInfo; hold: HoldControl; close(): void }) {
  const phase = phaseOf(held, hold.closed ?? undefined);
  if (phase !== "open") return <footer className="ap-drawer-foot"><Button small tone="quiet" onClick={close}>Close</Button></footer>;
  const reason = approveReason(held, hold);
  const idle = hold.busy;
  // Approve once is the desktop app's. A browser tab does not offer it; a remote window shows it off, with the reason.
  const offered = hold.desktop || hold.remote;
  const approveOff = idle || hold.asking || hold.remote || reason !== null;
  const answerOff = idle || hold.remote;
  return (
    <footer className="ap-drawer-foot">
      {offered ? <Button small tone="primary" disabled={approveOff} title={reason ?? undefined} aria-description={reason ?? undefined} onClick={hold.approve}>Approve once</Button> : null}
      <Button small disabled={answerOff} title={hold.remote ? HELD.remote : undefined} onClick={hold.deny}>Deny</Button>
      <Button small tone="quiet" disabled={answerOff} title={hold.remote ? HELD.remote : undefined} onClick={hold.dismiss}>Dismiss</Button>
      {reason ? <small className="ap-muted ap-reason">{reason}</small> : null}
      {hold.error ? <small className="ap-muted ap-reason" role="alert">{hold.error}</small> : null}
    </footer>
  );
}
