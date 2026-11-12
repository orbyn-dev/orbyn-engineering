// The held-command card: what the drawer shows for a command the guard holds for you, and the dialog behind
// Approve once. The words and states are the Crew designs' (v31): approval-confirm, -wrong, -network, -noaccount,
// approval-expired and approval-decided.
import { Info } from "lucide-react";
import { Button } from "../../../../ui/controls";
import { ConfirmItsYou } from "../../../account";
import { GUARD_WORKERS, HELD, MODE_COPY } from "./copy";
import { decidedLine, phaseOf, type HoldControl } from "./held";
import type { ApprovalRow, HeldInfo, Mode } from "./model";
import { Facts } from "./parts";

const UNREADABLE = "This command is too long to read whole, so it cannot be approved once.";

const confirmId = (id: string) => `crew-approve-once:${id}`;

function Said({ children, note }: { children: string; note?: string | null }) {
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
  if (!held.readable) return UNREADABLE;
  return null;
}

export function HeldBody({ row, held, hold, mode }: { row: ApprovalRow; held: HeldInfo; hold: HoldControl; mode: Mode }) {
  const phase = phaseOf(held, hold.closed ?? undefined);
  const decided = decidedLine(held.state, held.decidedAt);
  const none = hold.standing === "none";
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
      {phase === "open" && none ? (
        <section className="ap-box" data-quiet>
          <div className="ap-said"><Info size={14} aria-hidden /><span>{HELD.noAccount}</span></div>
          <p>{HELD.noAccountNote}</p>
          <div><Button small onClick={hold.signIn}>Sign in</Button></div>
        </section>
      ) : null}
      {phase === "open" && !none ? (
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
  const approveOff = idle || hold.standing !== "signed_in" || reason !== null;
  const answerOff = idle || hold.remote;
  return (
    <footer className="ap-drawer-foot">
      <Button small tone="primary" disabled={approveOff} title={reason ?? undefined} aria-description={reason ?? undefined} onClick={hold.approve}>Approve once</Button>
      <Button small disabled={answerOff} title={hold.remote ? HELD.remote : undefined} onClick={hold.deny}>Deny</Button>
      <Button small tone="quiet" disabled={answerOff} title={hold.remote ? HELD.remote : undefined} onClick={hold.dismiss}>Dismiss</Button>
      {reason ? <small className="ap-muted ap-reason">{reason}</small> : null}
      {hold.error ? <small className="ap-muted ap-reason" role="alert">{hold.error}</small> : null}
    </footer>
  );
}

/** "Confirm it's you" for one held command: the password, then the nonce goes to the route once. */
export function ApproveOnceDialog({ row, approve, onClose }: { row: ApprovalRow; approve(id: string, nonce: string): Promise<boolean>; onClose(): void }) {
  return (
    <ConfirmItsYou
      action="approve_device"
      deviceId={confirmId(row.id)}
      confirmLabel="Approve once"
      networkNote={HELD.networkNote}
      cancelOnEmptyEnter
      subject={
        <>
          <Facts rows={[{ label: "Asked by", text: `Task ${row.taskId ?? ""}`.trim() }]} />
          <pre className="ap-cmd"><code>{row.block}</code></pre>
        </>
      }
      onConfirmed={(nonce) => approve(row.id, nonce)}
      onClose={onClose}
    />
  );
}
