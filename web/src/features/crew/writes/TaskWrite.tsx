import { useRef, useState, type ReactNode } from "react";
import { useConnection } from "../../../product/state/connection";
import { Button } from "../../../ui/controls";
import { Dialog } from "../../../ui/Dialog";
import { REASON_MAX } from "./api";
import { rerunBusy, runWrite, useWrites, type Op, type WriteKind } from "./store";
import "./writes.css";

const COPY: Record<WriteKind, { title: string; body: string; confirm: string; tone: "danger" | "primary" }> = {
  cancel: { title: "Cancel this task?", body: "Its worker is stopped and the task ends as cancelled.", confirm: "Cancel task", tone: "danger" },
  rerun: { title: "Re-run this task’s checks?", body: "This runs the checks again on this computer and can take up to 30 minutes.", confirm: "Re-run checks", tone: "primary" },
};

/** Why an action is off, or null when it can be chosen. */
export interface Can {
  enabled: boolean;
  why: string | null;
}

/**
 * Cancel and Re-run checks for one task, shared by the Taskboard card and the Flow zoom. `allowCancel` and `allowRerun` come from the
 * card's advisory flags (the service decides, and refuses what it must). Choosing an action opens a confirm; nothing is sent before
 * its confirm button is pressed, and no path but that click sends anything.
 */
export function useTaskWrite({ id, name, allowCancel, allowRerun, onDone }: { id: string; name: string; allowCancel: boolean; allowRerun: boolean; onDone?(): void }) {
  const op = useWrites((state) => state.ops[id]);
  const anyRerun = useWrites((state) => rerunBusy(state.ops));
  const remote = useConnection((state) => state.remote);
  const [asking, setAsking] = useState<WriteKind | null>(null);

  const can = (kind: WriteKind): Can => {
    if (remote) return { enabled: false, why: "Not available from another computer" };
    if (op?.phase === "busy") return { enabled: false, why: "Waiting for the answer" };
    if (kind === "rerun" && anyRerun) return { enabled: false, why: "Another re-run is running" };
    return (kind === "cancel" ? allowCancel : allowRerun) ? { enabled: true, why: null } : { enabled: false, why: null };
  };

  const dialog = asking ? <ConfirmWrite kind={asking} name={name} onClose={() => setAsking(null)} onConfirm={(reason) => { setAsking(null); void runWrite(asking, id, reason, onDone); }} /> : null;
  return { op, can, ask: setAsking, dialog };
}

function ConfirmWrite({ kind, name, onClose, onConfirm }: { kind: WriteKind; name: string; onClose(): void; onConfirm(reason: string): void }) {
  const copy = COPY[kind];
  const [reason, setReason] = useState("");
  // A second press while the first is being handled sends nothing.
  const sent = useRef(false);
  return (
    <Dialog
      title={copy.title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} data-dialog-focus>
            Not now
          </Button>
          <Button
            tone={copy.tone}
            onClick={() => {
              if (sent.current) return;
              sent.current = true;
              onConfirm(reason);
            }}
          >
            {copy.confirm}
          </Button>
        </>
      }
    >
      {name ? <p className="tw-name">{name}</p> : null}
      <p>{copy.body}</p>
      <label className="tw-reason">
        Reason (optional)
        <textarea value={reason} maxLength={REASON_MAX} rows={2} onChange={(event) => setReason(event.target.value)} />
      </label>
    </Dialog>
  );
}

/** The one line a task's last write leaves: busy, answered with counts, or refused with the service's own words. Plain text. */
export function WriteStatus({ id, op, children }: { id: string; op: Op | undefined; children?: ReactNode }) {
  const dismiss = useWrites((state) => state.dismiss);
  if (!op) return null;
  if (op.phase === "busy") {
    return (
      <p className="tw-status" role="status">
        {op.kind === "cancel" ? "Cancelling…" : "Re-running the checks. This can take up to 30 minutes."}
        {children}
      </p>
    );
  }
  const close = (
    <button type="button" className="tw-dismiss" onClick={() => dismiss(id)}>
      Dismiss
    </button>
  );
  if (op.phase === "failed") {
    return (
      <div className="tw-status tw-bad" role="alert">
        <span>{op.message}</span>
        {op.kind === "cancel" && op.status === 502 ? <span>{/cancelled, but/i.test(op.message) ? "The task is cancelled." : "The task was not cancelled."}</span> : null}
        {close}
      </div>
    );
  }
  if (op.kind === "cancel") {
    return (
      <p className="tw-status" role="status">
        {op.answer.already ? "Already cancelled." : "Cancelled."} {close}
      </p>
    );
  }
  const a = op.answer;
  return (
    <div className="tw-status" role="status">
      <span>
        Checks: {a.ran} of {a.total} ran · {a.passed} passed{a.refused > 0 ? ` · ${a.refused} refused` : ""}{a.skipped > 0 ? ` · ${a.skipped} skipped` : ""} · {a.gate === "pass" ? "verdict pass" : a.gate === "fail" ? "verdict fail" : "no verdict"}
      </span>
      {a.refused_checks && a.refused_checks.length > 0 ? <span>Refused: {a.refused_checks.join(", ")}</span> : null}
      {close}
    </div>
  );
}
