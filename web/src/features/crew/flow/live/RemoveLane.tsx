import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { create } from "zustand";
import type { CrewRequiredReviewLane } from "../../../../generated/CrewRequiredReviewLane";
import type { CrewLaneRemovalReceipt } from "../../../../generated/CrewLaneRemovalReceipt";
import { useConnection } from "../../../../product/state/connection";
import { useAsync } from "../../../../lib/useAsync";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { reviewControlApi, controlScope, assertControlScope } from "../../shared/reviewControlApi";
import "../../autopilot/approvals/review-controls.css";

type Op = { phase: "busy" | "checking" | "unknown"; reason: string } | { phase: "failed"; reason: string; message: string } | { phase: "done"; reason: string; receipt: CrewLaneRemovalReceipt };
const useRemovalWrites = create<{ ops: Record<string, Op> }>(() => ({ ops: {} }));
const record = (key: string, op: Op | null) => useRemovalWrites.setState((s) => {
  const ops = { ...s.ops }; if (op) ops[key] = op; else delete ops[key];
  for (const [id, old] of Object.entries(ops)) { if (Object.keys(ops).length <= 128) break; if (id !== key && ["done", "failed"].includes(old.phase)) delete ops[id]; }
  return { ops };
});
const title = (lane: string) => `${lane.charAt(0).toUpperCase()}${lane.slice(1)} review`;
export const reasonError = (reason: string): string | null => !reason.trim() ? "Enter a reason to remove this lane." : [...reason.trim()].length > 1000 ? "Keep the reason to 1,000 characters or fewer." : null;

export function RequiredLanes({ task, name, onChanged }: { task: string; name: string; onChanged(): Promise<void> | void }) {
  const scope = controlScope();
  const read = useAsync(`required-review-lanes:${scope}:${task}`, (signal) => reviewControlApi.lanes(task, signal));
  if (!read.data) return read.status === "error" ? <p role="alert">Required lanes could not be read. <Button small onClick={() => void read.reload()}>Reload task</Button></p> : null;
  if (read.data.task !== task) return <p role="alert">Required lanes could not be read. <Button small onClick={() => void read.reload()}>Reload task</Button></p>;
  return <section className="fv-required" aria-label="Required review lanes">
    {read.data.error ? <p role="alert">{read.data.error} <Button small onClick={() => void read.reload()}>Reload task</Button></p> : null}
    {read.data.lanes.map((lane) => <RemoveLane key={`${task}:${lane.lane}`} task={task} name={name} lane={lane} onChanged={async () => { assertControlScope(scope); const fresh = await reviewControlApi.lanes(task); assertControlScope(scope); if (fresh.task !== task) throw new Error("Wrong task"); read.setData(fresh); await onChanged(); }} />)}
  </section>;
}

function RemoveLane({ task, name, lane, onChanged }: { task: string; name: string; lane: CrewRequiredReviewLane; onChanged(): Promise<void> }) {
  const scope = controlScope(); const key = `${scope}|${task}|${lane.lane}`;
  const op = useRemovalWrites((s) => s.ops[key]);
  const remote = useConnection((s) => s.remote);
  const [open, setOpen] = useState(false), [reason, setReason] = useState(""), [error, setError] = useState<string | null>(null);
  const [refreshFailed, setRefreshFailed] = useState(false);
  const [blocked, setBlocked] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null), receiptNode = useRef<HTMLParagraphElement>(null);
  const alive = useRef(true), current = useRef(key);
  const focusReceipt = useRef(false);
  useLayoutEffect(() => { current.current = key; }, [key]);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const mine = () => alive.current && current.current === key && controlScope() === scope;
  const receipt = lane.removal ?? (op?.phase === "done" ? op.receipt : null);
  useEffect(() => { if (receipt && focusReceipt.current) { focusReceipt.current = false; receiptNode.current?.focus(); } }, [receipt]);
  const busy = op?.phase === "busy" || op?.phase === "checking";
  const unknown = op?.phase === "unknown";
  const finish = async (actual: CrewLaneRemovalReceipt, submitted: string) => {
    if (mine()) focusReceipt.current = true;
    record(key, { phase: "done", reason: submitted, receipt: actual });
    if (!mine()) return;
    setOpen(false);
    try { await onChanged(); } catch { if (mine()) setRefreshFailed(true); }
  };
  const reconcile = async (submitted: string) => {
    if (useRemovalWrites.getState().ops[key]?.phase === "checking") return;
    record(key, { phase: "checking", reason: submitted });
    try {
      assertControlScope(scope);
      const fresh = await reviewControlApi.lanes(task);
      const freshLane = fresh.lanes.find((l) => l.lane === lane.lane);
      const actual = freshLane?.removal;
      if (fresh.task !== task) throw new Error("Wrong task receipt");
      if (actual) await finish(actual, submitted);
      else if (fresh.error) record(key, { phase: "unknown", reason: submitted });
      else { if (mine()) setBlocked(!freshLane?.can_remove); record(key, { phase: "failed", reason: submitted, message: freshLane?.locked ? "This lane is locked and cannot be removed. Reload task." : "Couldn’t remove this lane. Your reason is still here. Try again." }); }
    } catch { record(key, { phase: "unknown", reason: submitted }); }
  };
  const submit = async () => {
    const invalid = reasonError(reason); setError(invalid);
    if (invalid) { field.current?.focus(); return; }
    if (remote || blocked || !lane.can_remove || lane.locked || receipt || ["busy", "checking", "unknown"].includes(useRemovalWrites.getState().ops[key]?.phase ?? "")) return;
    const submitted = reason.trim(); record(key, { phase: "busy", reason: submitted });
    try {
      assertControlScope(scope);
      const answer = await reviewControlApi.remove(task, lane.lane, submitted);
      if (answer.task !== task || answer.receipt.lane !== lane.lane) throw new Error("Wrong lane receipt");
      await finish(answer.receipt, submitted);
    } catch { await reconcile(submitted); }
  };
  const cancel = () => { if (busy || unknown) return; record(key, null); setOpen(false); setReason(""); setError(null); };
  return <div className="fv-required-row" aria-busy={busy}>
    <div><b>{title(lane.lane)}{receipt ? " · Removed by person" : ""}</b><small>{receipt ? "Removed" : lane.locked ? "Locked" : "Required"} · {lane.source}</small>
      {receipt ? <p ref={receiptNode} tabIndex={-1}>Recorded reason: {receipt.reason}</p> : null}
      {remote && !lane.locked && !receipt ? <p>Only a person on this computer can remove a required lane.</p> : null}
      {refreshFailed ? <p role="alert">Lane removed. Couldn’t refresh this task. <Button small onClick={() => void onChanged().then(() => { if (mine()) setRefreshFailed(false); }).catch(() => { if (mine()) setRefreshFailed(true); })}>Reload task</Button></p> : null}
    </div>
    {!receipt && !remote && !lane.locked && lane.can_remove ? <Button small tone="quiet" disabled={busy} onClick={() => { setReason(op?.reason ?? ""); setOpen(true); }}>{unknown ? "Check again" : busy ? "Removing…" : "Remove…"}</Button> : null}
    {open && !receipt ? <Dialog title={`Remove ${title(lane.lane)}?`} className="fv-remove-dialog" busy={busy || unknown} onClose={cancel} footer={unknown ? <Button small onClick={() => void reconcile(op?.reason ?? reason)}>Check again</Button> : busy ? <><Button small disabled>Cancel</Button><Button small tone="danger" disabled>{op?.phase === "checking" ? "Checking…" : "Removing…"}</Button></> : <><Button small onClick={cancel}>Cancel</Button>{blocked ? <Button small onClick={() => void onChanged().catch(() => { if (mine()) setError("Couldn’t reload this task."); })}>Reload task</Button> : <Button small tone="danger" disabled={remote || !lane.can_remove} onClick={() => void submit()}>Remove lane</Button>}</>}>
      <small>Task: {name}</small>
      {op?.phase === "checking" ? <><p role="status">Checking whether the lane was removed…</p><small>Your reason is kept while we check.</small></> : unknown ? <p role="alert">Couldn’t check whether the lane was removed. Your reason is still here.</p> : <>
        <p>This removes the required lane for this task. Existing findings stay open.</p>
        <label htmlFor={`remove-${lane.lane}`}>Reason</label><textarea ref={field} data-dialog-focus id={`remove-${lane.lane}`} value={reason} disabled={busy} aria-invalid={!!error} aria-describedby={`remove-${lane.lane}-hint${error ? ` remove-${lane.lane}-error` : ""}`} onChange={(event) => { setReason(event.target.value); if (!reasonError(event.target.value)) setError(null); }} />
        <small id={`remove-${lane.lane}-hint`}>Recorded with this task. 1–1,000 characters.</small>
        {error ? <p role="alert" id={`remove-${lane.lane}-error`}>{error}</p> : null}
        {op?.phase === "busy" ? <p role="status">Removing {title(lane.lane)}…</p> : null}
        {op?.phase === "failed" ? <p role="alert">{op.message}</p> : null}
      </>}
    </Dialog> : null}
  </div>;
}
